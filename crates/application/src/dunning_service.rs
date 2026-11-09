use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CalendarDate, CompanyId, Contact, Decimal, DunningChannel,
    DunningPlan, DunningRule, Email, EntryId, Notification, NotificationId, NotificationStatus,
    Party, dunning_plan_changes, dunning_plan_creation_changes,
};

use crate::audit::audit_entry;
use crate::{
    ApplicationError, Claim, DunningPlanRepository, EmailContent, EmailSender,
    FinancialEntryRepository, IdentityRepository, InstallmentLinks, MessageSender,
    NotificationRepository, OverdueItem, PartyRepository, TextMessage,
};

/// How many days late a reminder may still go out: the pass is hourly, but the server may have been down.
const CATCH_UP_DAYS: i64 = 2;

/// What one pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DunningReport {
    pub sent: usize,
    pub failed: usize,
    /// Installments a rule reached whose customer has nowhere to send it by that channel.
    pub without_recipient: usize,
}

/// The reminders of a company's receivables ([ADR 0048]): the plan (when to remind, by which channel) and the
/// pass that sends what is due, once each however often it runs.
pub struct DunningService<D, E, P, K, N, I, M, W> {
    plans: D,
    entries: E,
    parties: P,
    links: K,
    notifications: N,
    identity: I,
    emails: M,
    messages: W,
}

impl<D, E, P, K, N, I, M, W> DunningService<D, E, P, K, N, I, M, W>
where
    D: DunningPlanRepository,
    E: FinancialEntryRepository,
    P: PartyRepository,
    K: InstallmentLinks,
    N: NotificationRepository,
    I: IdentityRepository,
    M: EmailSender,
    W: MessageSender,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plans: D,
        entries: E,
        parties: P,
        links: K,
        notifications: N,
        identity: I,
        emails: M,
        messages: W,
    ) -> Self {
        Self {
            plans,
            entries,
            parties,
            links,
            notifications,
            identity,
            emails,
            messages,
        }
    }

    /// Whether the operator configured a WhatsApp provider: without one a rule for it is never tried.
    pub fn whatsapp_is_available(&self) -> bool {
        self.messages.is_enabled()
    }

    /// The company's plan; one that was never made is off and has no rules.
    pub async fn plan(&self, company_id: CompanyId) -> Result<DunningPlan, ApplicationError> {
        match self.plans.find(company_id).await? {
            Some(plan) => Ok(plan),
            None => Ok(DunningPlan::new(company_id, false, Vec::new())?),
        }
    }

    /// Replaces the plan, audited in the name of `actor`. One that changes nothing writes nothing.
    pub async fn replace_plan(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        enabled: bool,
        rules: Vec<DunningRule>,
    ) -> Result<DunningPlan, ApplicationError> {
        let before = self.plans.find(company_id).await?;
        let after = DunningPlan::new(company_id, enabled, rules)?;
        let (action, changes) = match &before {
            Some(stored) if *stored == after => return Ok(after),
            Some(stored) => (AuditAction::Updated, dunning_plan_changes(stored, &after)),
            None => (AuditAction::Created, dunning_plan_creation_changes(&after)),
        };
        let audit = audit_entry(&after, actor, action, changes);
        self.plans.save(&after, &audit).await?;
        Ok(after)
    }

    pub async fn plan_history(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        Ok(self.plans.history(company_id).await?)
    }

    /// The reminders about an entry of the company, oldest first.
    pub async fn notifications(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Notification>, ApplicationError> {
        self.entries
            .find(company_id, entry_id)
            .await?
            .ok_or(ApplicationError::EntryNotFound)?;
        Ok(self
            .notifications
            .list_by_entry(company_id, entry_id)
            .await?)
    }

    /// The pass: for every company whose plan is on, sends the reminders that `day` calls for. A rule reaches
    /// the installments whose due day moved by its days is `day` or one of the two days before (so a stopped
    /// server catches up); each reminder is claimed in the store first, so running the pass again, or two
    /// at once, sends nothing twice. A company that fails is counted and the others go on.
    pub async fn send_due(&self, day: CalendarDate) -> Result<DunningReport, ApplicationError> {
        let mut report = DunningReport::default();
        for company_id in self.plans.enabled_companies().await? {
            if let Err(error) = self.send_for(company_id, day, &mut report).await {
                tracing::error!(%error, "sending a company's reminders failed");
                report.failed += 1;
            }
        }
        Ok(report)
    }

    async fn send_for(
        &self,
        company_id: CompanyId,
        day: CalendarDate,
        report: &mut DunningReport,
    ) -> Result<(), ApplicationError> {
        let Some(plan) = self
            .plans
            .find(company_id)
            .await?
            .filter(DunningPlan::is_enabled)
        else {
            return Ok(());
        };
        let company_name = self
            .identity
            .company_name(company_id)
            .await?
            .unwrap_or_default();
        for rule in plan.rules() {
            if rule.channel == DunningChannel::Whatsapp && !self.messages.is_enabled() {
                continue;
            }
            let Some(until) = shifted(day, -i64::from(rule.days)) else {
                continue;
            };
            let Some(since) = shifted(until, -CATCH_UP_DAYS) else {
                continue;
            };
            for item in self
                .entries
                .receivable_installments_due(company_id, since, until)
                .await?
            {
                // One installment that cannot be reminded of must not keep the later ones waiting.
                if let Err(error) = self
                    .remind(company_id, &company_name, *rule, &item, day, report)
                    .await
                {
                    tracing::error!(%error, "a reminder could not be sent");
                    report.failed += 1;
                }
            }
        }
        Ok(())
    }

    async fn remind(
        &self,
        company_id: CompanyId,
        company_name: &str,
        rule: DunningRule,
        item: &OverdueItem,
        day: CalendarDate,
        report: &mut DunningReport,
    ) -> Result<(), ApplicationError> {
        let Some(party_id) = item.party_id else {
            report.without_recipient += 1;
            return Ok(());
        };
        let party = self
            .parties
            .find(company_id, party_id)
            .await?
            .ok_or(ApplicationError::PartyNotFound)?;
        let address = match rule.channel {
            DunningChannel::Email => party
                .address_to_write_to()
                .map(|email| email.as_str().to_owned()),
            DunningChannel::Whatsapp => phone_of(&party),
        };
        let Some(address) = address else {
            report.without_recipient += 1;
            return Ok(());
        };
        let candidate = Notification::new(
            NotificationId::generate(),
            company_id,
            item.entry_id,
            item.installment,
            rule.days,
            rule.channel,
            &address,
        );
        let notification = match self.notifications.claim(&candidate).await? {
            Claim::New(notification) | Claim::Retry(notification) => notification,
            Claim::Done => return Ok(()),
        };
        // The link is made only for the reminder that is going to be sent. Without it the reminder would go
        // out with nothing to pay with, so a link that cannot be made fails the try, to be tried again.
        let link = match self
            .links
            .link_for_installment(company_id, item.entry_id, item.installment)
            .await
        {
            Ok(link) => link,
            Err(error) => {
                tracing::warn!(%error, "a payment link could not be made for a reminder");
                report.failed += 1;
                self.notifications
                    .finish(
                        company_id,
                        notification.id(),
                        NotificationStatus::Failed,
                        Some("the payment link could not be made"),
                    )
                    .await?;
                return Ok(());
            }
        };
        let text = ReminderText {
            company: company_name,
            customer: party.name(),
            description: &clipped(&item.description),
            installment: item.installment,
            owed: item.amount - item.settled,
            due: item.due_date,
            // Said by how far the due date really is, not by the rule: the pass catches up two days.
            days: i16::try_from(day.to_unix_days() - item.due_date.to_unix_days()).unwrap_or(0),
            link: link.as_deref(),
        };
        let sent = match rule.channel {
            DunningChannel::Email => match Email::parse(&address) {
                Ok(to) => self
                    .emails
                    .send_content(to, text.subject(), &text.content())
                    .await
                    .is_ok(),
                Err(_) => false,
            },
            DunningChannel::Whatsapp => self
                .messages
                .send(&TextMessage {
                    to: address,
                    text: text.one_line(),
                })
                .await
                .is_ok(),
        };
        let (status, note) = if sent {
            report.sent += 1;
            (NotificationStatus::Sent, None)
        } else {
            report.failed += 1;
            (NotificationStatus::Failed, Some("the message was not sent"))
        };
        self.notifications
            .finish(company_id, notification.id(), status, note)
            .await?;
        Ok(())
    }
}

/// A description in a reminder: the provider's template parameter is short.
fn clipped(text: &str) -> String {
    text.chars().take(120).collect()
}

/// `day` moved by `by` days, if it is still a day.
fn shifted(day: CalendarDate, by: i64) -> Option<CalendarDate> {
    CalendarDate::from_unix_days(day.to_unix_days() + by)
}

/// Where to send a WhatsApp reminder: the first contact that has a phone, as digits with the country code
/// (a Brazilian number written with 10 or 11 digits gets 55).
fn phone_of(party: &Party) -> Option<String> {
    let phone = party.contacts().iter().find_map(Contact::phone)?;
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    // A number written with `+` already has its country code; only a Brazilian-looking one without it (10 or
    // 11 digits, no trunk 0) is given 55.
    let local = !phone.trim_start().starts_with('+')
        && matches!(digits.len(), 10 | 11)
        && !digits.starts_with('0');
    Some(if local { format!("55{digits}") } else { digits })
}

/// What a reminder says, in Portuguese: the same words by e-mail and by WhatsApp.
struct ReminderText<'a> {
    company: &'a str,
    customer: &'a str,
    description: &'a str,
    installment: u16,
    owed: Decimal,
    due: CalendarDate,
    days: i16,
    link: Option<&'a str>,
}

impl ReminderText<'_> {
    fn when(&self) -> String {
        match self.days {
            0 => "vence hoje".to_owned(),
            1 => "venceu ontem".to_owned(),
            -1 => "vence amanhã".to_owned(),
            days if days < 0 => format!("vence em {} dias", -days),
            days => format!("venceu há {days} dias"),
        }
    }

    fn subject(&self) -> String {
        format!(
            "{}: parcela {} ({})",
            one_line(self.company),
            self.installment,
            self.when()
        )
    }

    fn content(&self) -> EmailContent {
        let company = one_line(self.company);
        let mut content = EmailContent::new(
            format!("A parcela {} {}", self.installment, self.when()),
            format!("Você recebeu este lembrete de {company} por uma parcela em aberto."),
        )
        .company(company.clone())
        .paragraph(format!("Olá, {}.", one_line(self.customer)))
        .paragraph(format!(
            "Passando para lembrar: a parcela {} de \"{}\" ({company}) {}.",
            self.installment,
            one_line(self.description),
            self.when(),
        ))
        .details(vec![
            ("Valor".to_owned(), brl(self.owed)),
            ("Vencimento".to_owned(), brazilian_date(self.due)),
        ]);
        if let Some(link) = self.link {
            content = content.button("Pagar agora", link);
        }
        content.note("Se você já pagou, desconsidere esta mensagem.")
    }

    fn one_line(&self) -> String {
        let mut text = format!(
            "{}: olá, {}! A parcela {} de \"{}\" {}. Valor {}, vencimento {}.",
            one_line(self.company),
            one_line(self.customer),
            self.installment,
            one_line(self.description),
            self.when(),
            brl(self.owed),
            brazilian_date(self.due),
        );
        if let Some(link) = self.link {
            text.push_str(&format!(" Pague em {link}"));
        }
        text
    }
}

/// A name or description as one line, so it cannot break the message it is put in.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn brazilian_date(day: CalendarDate) -> String {
    format!("{:02}/{:02}/{}", day.day(), day.month(), day.year())
}

/// `R$ 1.234,56`.
fn brl(amount: Decimal) -> String {
    let fixed = format!("{:.2}", amount.round_dp(2));
    let (whole, cents) = fixed.split_once('.').unwrap_or((&fixed, "00"));
    let mut grouped = String::new();
    for (index, digit) in whole.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(digit);
    }
    format!("R$ {},{cents}", grouped.chars().rev().collect::<String>())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn formats_money_and_dates_the_brazilian_way() {
        assert_eq!(brl(Decimal::from_str("1234.5").unwrap()), "R$ 1.234,50");
        assert_eq!(brl(Decimal::from_str("0.05").unwrap()), "R$ 0,05");
        assert_eq!(
            brl(Decimal::from_str("1000000").unwrap()),
            "R$ 1.000.000,00"
        );
        assert_eq!(
            brazilian_date(CalendarDate::parse("2026-12-09").unwrap()),
            "09/12/2026"
        );
    }

    #[test]
    fn a_plus_keeps_the_number_and_only_a_local_one_gets_the_country_code() {
        let party = |phone: &str| {
            Party::new(
                conta_verde_domain::PartyId::generate(),
                CompanyId::generate(),
                conta_verde_domain::PartyDraft {
                    name: "Ana".into(),
                    is_customer: true,
                    contacts: vec![conta_verde_domain::ContactDraft {
                        name: "Ana".into(),
                        phone: Some(phone.into()),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            )
            .unwrap()
        };
        assert_eq!(
            phone_of(&party("(81) 99999-0000")).unwrap(),
            "5581999990000"
        );
        assert_eq!(
            phone_of(&party("+55 81 99999-0000")).unwrap(),
            "5581999990000"
        );
        assert_eq!(
            phone_of(&party("+1 (415) 555-2671")).unwrap(),
            "14155552671"
        );
        assert_eq!(phone_of(&party("081 99999-0000")).unwrap(), "081999990000");
    }

    #[test]
    fn says_when_in_words() {
        let text = |days| ReminderText {
            company: "Padaria",
            customer: "Ana",
            description: "Venda 12",
            installment: 1,
            owed: Decimal::from(100),
            due: CalendarDate::parse("2026-12-10").unwrap(),
            days,
            link: None,
        };
        assert_eq!(text(-3).when(), "vence em 3 dias");
        assert_eq!(text(-1).when(), "vence amanhã");
        assert_eq!(text(0).when(), "vence hoje");
        assert_eq!(text(1).when(), "venceu ontem");
        assert_eq!(text(7).when(), "venceu há 7 dias");
        let body = text(7).content().to_text();
        assert!(body.contains("Valor: R$ 100,00"));
        // Without an open charge there is nothing to pay with, so there is no button and no link.
        assert!(!body.contains("Pagar agora") && !body.contains("https://"));
    }
}
