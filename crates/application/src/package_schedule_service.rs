use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CalendarDate, CompanyId, Email, PackageSchedule, Role,
    month_to_package, package_schedule_changes, package_schedule_creation_changes,
};

use crate::audit::audit_entry;
use crate::text::one_line;
use crate::{
    ApplicationError, EmailContent, EmailSender, IdentityRepository, PackageScheduleRepository,
};

/// What one pass of the monthly package did, for the log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackageReport {
    /// Messages queued, one per accountant.
    pub sent: usize,
    /// Companies whose month was already announced (by an earlier pass or another instance).
    pub already_sent: usize,
    /// Companies that are due and have no accountant: nothing is sent, and nothing is claimed, so inviting one
    /// later in the month still announces it.
    pub without_accountant: usize,
    /// Messages that could not be queued.
    pub failed: usize,
}

/// The schedule of the monthly package and the pass that announces it ([ADR 0067], [ADR 0072]).
pub struct PackageScheduleService<R, I, E> {
    repository: R,
    identities: I,
    emails: E,
    public_url: String,
}

impl<R, I, E> PackageScheduleService<R, I, E>
where
    R: PackageScheduleRepository,
    I: IdentityRepository,
    E: EmailSender,
{
    pub fn new(repository: R, identities: I, emails: E, public_url: &str) -> Self {
        Self {
            repository,
            identities,
            emails,
            public_url: public_url.trim_end_matches('/').to_owned(),
        }
    }

    /// The company's schedule; a company that never chose has it off.
    pub async fn get(&self, company_id: CompanyId) -> Result<PackageSchedule, ApplicationError> {
        Ok(self
            .repository
            .find(company_id)
            .await?
            .unwrap_or_else(|| PackageSchedule::off(company_id)))
    }

    /// Chooses whether the package is announced and on which day of the month. The same choice again writes nothing.
    pub async fn replace(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        enabled: bool,
        day: u8,
    ) -> Result<PackageSchedule, ApplicationError> {
        let stored = self.repository.find(company_id).await?;
        let before = stored.unwrap_or_else(|| PackageSchedule::off(company_id));
        let after = PackageSchedule::new(company_id, enabled, day)?;
        // The same choice writes nothing, but a company that never chose and chooses what it has is a choice.
        if stored.is_some() && before == after {
            return Ok(after);
        }
        let (action, changes) = match stored {
            Some(_) => (
                AuditAction::Updated,
                package_schedule_changes(&before, &after),
            ),
            None => (
                AuditAction::Created,
                package_schedule_creation_changes(&after),
            ),
        };
        let audit = audit_entry(&after, actor, action, changes);
        self.repository.save(&after, &audit).await?;
        Ok(after)
    }

    /// What was recorded about the company's schedule, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        Ok(self.repository.history(company_id).await?)
    }

    /// How many members the announcement would reach: the ones whose role is `accountant`.
    pub async fn accountants(&self, company_id: CompanyId) -> Result<usize, ApplicationError> {
        Ok(self.recipients(company_id).await?.len())
    }

    /// The pass of the hourly task: for every company whose schedule is due on `today`, announces the previous month
    /// to its accountants, once. The month is claimed in one statement before anything is queued; a pass that queued
    /// nothing gives the claim back, so the next one tries again.
    pub async fn send_due(
        &self,
        today: CalendarDate,
        now_millis: i64,
    ) -> Result<PackageReport, ApplicationError> {
        let month = month_to_package(today);
        let mut report = PackageReport::default();
        for company_id in self.repository.due(today.day()).await? {
            let recipients = self.recipients(company_id).await?;
            if recipients.is_empty() {
                report.without_accountant += 1;
                continue;
            }
            if !self.repository.claim(company_id, month, now_millis).await? {
                report.already_sent += 1;
                continue;
            }
            let company_name = self
                .identities
                .company_name(company_id)
                .await?
                .unwrap_or_default();
            let (subject, content) = self.message(company_id, &company_name, month);
            let mut queued = 0;
            for to in recipients {
                match self
                    .emails
                    .send_content(to, subject.clone(), &content)
                    .await
                {
                    Ok(()) => queued += 1,
                    Err(_) => {
                        tracing::warn!(
                            company = %company_id.as_uuid(),
                            "the monthly package could not be announced to an accountant"
                        );
                        report.failed += 1;
                    }
                }
            }
            report.sent += queued;
            // Some may have gone out, so the month stays claimed then: a second pass would send them twice.
            if queued == 0 {
                self.repository.release(company_id, month).await?;
            }
        }
        Ok(report)
    }

    async fn recipients(&self, company_id: CompanyId) -> Result<Vec<Email>, ApplicationError> {
        Ok(self
            .identities
            .members_of(company_id)
            .await?
            .into_iter()
            .filter(|member| member.role == Role::Accountant)
            .map(|member| member.user.email().clone())
            .collect())
    }

    /// The e-mail: it says the month's package is ready and links to the Contador page, which asks for a login. It
    /// carries no figure and no file ([ADR 0072]).
    fn message(
        &self,
        company_id: CompanyId,
        company_name: &str,
        month: CalendarDate,
    ) -> (String, EmailContent) {
        let company_name = one_line(company_name);
        let label = month_label(month);
        let link = format!(
            "{}/companies/{}/accounting?month={:04}-{:02}",
            self.public_url,
            company_id.as_uuid(),
            month.year(),
            month.month()
        );
        let content = EmailContent::new(
            format!("O pacote de {label} está pronto"),
            format!(
                "Você recebeu este e-mail porque é contador de {company_name} no conta-verde e a empresa agendou este aviso todo mês."
            ),
        )
        .company(company_name.clone())
        .paragraph(format!(
            "Os arquivos de {label} de {company_name} já podem ser baixados: lançamentos, baixas, transferências e os arquivos anexados."
        ))
        .button("Abrir o pacote", link)
        .note("Para baixar é preciso entrar com a sua conta. Este e-mail não traz dados da empresa nem arquivos.");
        (
            format!("O pacote de {label} de {company_name} está pronto"),
            content,
        )
    }
}

/// `outubro de 2026`.
fn month_label(month: CalendarDate) -> String {
    const NAMES: [&str; 12] = [
        "janeiro",
        "fevereiro",
        "março",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ];
    format!(
        "{} de {}",
        NAMES[usize::from(month.month()) - 1],
        month.year()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_month_is_named_in_portuguese() {
        let day = |raw: &str| CalendarDate::parse(raw).unwrap();
        assert_eq!(month_label(day("2026-10-01")), "outubro de 2026");
        assert_eq!(month_label(day("2027-03-01")), "março de 2027");
    }
}
