use conta_verde_domain::{
    Actor, COOLDOWN_MILLIS, CompanyId, DAILY_CAP, DAY_MILLIS, Decimal, DocumentMail,
    DocumentMailId, DocumentMailKind, Email, EntryId, EntryKind, Party, PartyId, PurchaseOrderId,
    PurchaseStatus, SettlementId,
};
use uuid::Uuid;

use crate::text::{br_date, one_line};
use crate::{
    ApplicationError, AuthError, DocumentMailRepository, EmailContent, EmailSender,
    FinancialEntryRepository, ItemRow, Items, PartyRepository, PurchaseOrderRepository,
    Reservation,
};

/// What goes out: to whom, under what subject, and what it says.
struct Draft {
    to: Email,
    subject: String,
    content: EmailContent,
}

/// An e-mail that was asked for: to whom, and whether it was taken for sending (queued, [ADR 0059]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailed {
    pub to: Email,
    pub emailed: bool,
}

/// Who asks for a document to be e-mailed, when, and where it should go.
pub struct MailAsk<'a> {
    pub actor: &'a Actor,
    /// Unix milliseconds: the limits are judged against it, so a test moves it.
    pub now_millis: i64,
    /// The address asked for; the party's own when left out.
    pub to: Option<&'a str>,
    pub company_name: &'a str,
}

/// Sends a person a document as text: a purchase order to its supplier, the proof of a payment to the customer who
/// made it ([ADR 0061]). The document does not change, so there is no audit entry; **what was sent is a log**
/// ([ADR 0065]): who sent it to whom and when, with a limit that is the same for everyone: the same document is not sent
/// again to the same address within five minutes, and a company may e-mail a hundred documents a day. A message that is
/// not taken (no mail server) is `emailed: false` and uses up nothing.
pub struct DocumentMailService<P, E, Y, M, L> {
    purchases: P,
    entries: E,
    parties: Y,
    emails: M,
    log: L,
}

impl<P, E, Y, M, L> DocumentMailService<P, E, Y, M, L>
where
    P: PurchaseOrderRepository,
    E: FinancialEntryRepository,
    Y: PartyRepository,
    M: EmailSender,
    L: DocumentMailRepository,
{
    pub fn new(purchases: P, entries: E, parties: Y, emails: M, log: L) -> Self {
        Self {
            purchases,
            entries,
            parties,
            emails,
            log,
        }
    }

    /// What was sent of a purchase order or of a payment's receipt, newest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        kind: DocumentMailKind,
        document_id: Uuid,
    ) -> Result<Vec<DocumentMail>, ApplicationError> {
        Ok(self
            .log
            .of_document(company_id, kind, document_id, 20)
            .await?)
    }

    /// Records the send under the limits, sends the message, and takes the record back if it went nowhere.
    async fn deliver(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        now_millis: i64,
        kind: DocumentMailKind,
        document_id: Uuid,
        draft: Draft,
    ) -> Result<Mailed, ApplicationError> {
        let Draft {
            to,
            subject,
            content,
        } = draft;
        // A document is mailed in the name of a person: a key does not reach this ([ADR 0087]).
        let sent_by = actor.user_id().ok_or(AuthError::NotAllowed)?;
        let mail = DocumentMail {
            id: DocumentMailId::generate(),
            company_id,
            kind,
            document_id,
            to: to.clone(),
            sent_by,
            sent_at_millis: now_millis,
        };
        match self
            .log
            .reserve(&mail, COOLDOWN_MILLIS, DAY_MILLIS, DAILY_CAP)
            .await?
        {
            Reservation::Reserved => {}
            Reservation::TooSoon => {
                return Err(ApplicationError::MailRefused(
                    "this document was sent to this address a moment ago: wait a few minutes",
                ));
            }
            Reservation::CapReached => {
                return Err(ApplicationError::MailRefused(
                    "the company reached the limit of documents e-mailed in a day: try again tomorrow",
                ));
            }
        }
        let emailed = self
            .emails
            .send_content(to.clone(), subject, &content)
            .await
            .is_ok();
        if !emailed {
            self.log.release(company_id, mail.id).await?;
        }
        Ok(Mailed { to, emailed })
    }

    /// E-mails an approved or partly received purchase order to `to`, or to its supplier's address.
    pub async fn send_purchase_order(
        &self,
        company_id: CompanyId,
        request: &MailAsk<'_>,
        id: PurchaseOrderId,
    ) -> Result<Mailed, ApplicationError> {
        let MailAsk {
            actor,
            now_millis,
            to,
            company_name,
        } = *request;
        let order = self
            .purchases
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::PurchaseNotFound)?;
        if !matches!(
            order.status(),
            PurchaseStatus::Approved | PurchaseStatus::PartiallyReceived
        ) {
            return Err(ApplicationError::CannotMail(
                "only an approved or partly received purchase order is sent to the supplier",
            ));
        }
        let supplier = self.party(company_id, Some(order.supplier_id())).await?;
        let to = recipient(to, supplier.as_ref())?;
        let company = one_line(company_name);
        let mut content = EmailContent::new(
            format!("Pedido de compra nº {}", order.number()),
            format!("Você recebeu este pedido de {company}, seu cliente."),
        )
        .company(company.clone())
        .paragraph(format!(
            "{company} enviou o pedido de compra nº {}.",
            order.number()
        ))
        .items(Items {
            title: "Itens".to_owned(),
            rows: order
                .lines()
                .iter()
                .map(|line| ItemRow {
                    quantity: quantity(line.quantity().as_decimal()),
                    description: one_line(line.description()),
                    price: format!("R$ {}", money(line.unit_price().as_decimal())),
                })
                .collect(),
            total: Some(("Total".to_owned(), format!("R$ {}", money(order.total())))),
        });
        let mut details = Vec::new();
        if let Some(day) = order.expected_on() {
            details.push(("Entrega prevista".to_owned(), br_date(day)));
        }
        if let Some(notes) = order.notes() {
            details.push(("Observações".to_owned(), one_line(notes)));
        }
        if !details.is_empty() {
            content = content.details(details);
        }
        self.deliver(
            company_id,
            actor,
            now_millis,
            DocumentMailKind::PurchaseOrder,
            id.as_uuid(),
            Draft {
                to,
                subject: format!("Pedido de compra nº {} de {company}", order.number()),
                content,
            },
        )
        .await
    }

    /// E-mails the customer the proof of one settlement of a receivable that was not reversed.
    pub async fn send_payment_receipt(
        &self,
        company_id: CompanyId,
        request: &MailAsk<'_>,
        entry_id: EntryId,
        settlement_id: SettlementId,
    ) -> Result<Mailed, ApplicationError> {
        let MailAsk {
            actor,
            now_millis,
            to,
            company_name,
        } = *request;
        let entry = self
            .entries
            .find(company_id, entry_id)
            .await?
            .ok_or(ApplicationError::EntryNotFound)?;
        if entry.kind() != EntryKind::Receivable {
            return Err(ApplicationError::CannotMail(
                "a payment receipt is sent to a customer for a receivable",
            ));
        }
        let settlement = self
            .entries
            .settlements_of(company_id, &[entry_id])
            .await?
            .into_iter()
            .find(|settlement| settlement.id() == settlement_id)
            .ok_or(ApplicationError::SettlementNotFound)?;
        if settlement.is_reversed() {
            return Err(ApplicationError::CannotMail(
                "the payment was reversed: there is no receipt to send",
            ));
        }
        let customer = self.party(company_id, entry.party_id()).await?;
        let to = recipient(to, customer.as_ref())?;
        let company = one_line(company_name);
        let mut rows = vec![
            ("Referente a".to_owned(), one_line(entry.description())),
            (
                "Parcela".to_owned(),
                format!(
                    "{} de {}",
                    settlement.installment(),
                    entry.installments().len()
                ),
            ),
            ("Data".to_owned(), br_date(settlement.settled_on())),
        ];
        for (label, amount) in [
            ("Juros", settlement.interest().as_decimal()),
            ("Multa", settlement.fine().as_decimal()),
            ("Desconto", settlement.discount().as_decimal()),
        ] {
            if !amount.is_zero() {
                rows.push((label.to_owned(), format!("R$ {}", money(amount))));
            }
        }
        rows.push((
            "Valor pago".to_owned(),
            format!("R$ {}", money(settlement.cash())),
        ));
        let mut content = EmailContent::new(
            "Comprovante de pagamento",
            format!("Você recebeu este comprovante de {company} por um pagamento registrado."),
        )
        .company(company.clone());
        if let Some(party) = customer.as_ref() {
            content = content.paragraph(format!("Olá, {}.", one_line(party.name())));
        } else {
            content = content.paragraph("Olá.");
        }
        let content = content
            .paragraph(format!(
                "{company} confirma o recebimento do seu pagamento."
            ))
            .details(rows)
            .note("Guarde este e-mail como comprovante.");
        self.deliver(
            company_id,
            actor,
            now_millis,
            DocumentMailKind::PaymentReceipt,
            settlement_id.as_uuid(),
            Draft {
                to,
                subject: format!("Comprovante de pagamento de {company}"),
                content,
            },
        )
        .await
    }

    async fn party(
        &self,
        company_id: CompanyId,
        id: Option<PartyId>,
    ) -> Result<Option<Party>, ApplicationError> {
        match id {
            Some(id) => Ok(self.parties.find(company_id, id).await?),
            None => Ok(None),
        }
    }
}

/// The address asked for, or the party's own when none was given.
fn recipient(given: Option<&str>, party: Option<&Party>) -> Result<Email, ApplicationError> {
    match given.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => Ok(Email::parse(text)?),
        None => party
            .and_then(Party::address_to_write_to)
            .cloned()
            .ok_or(ApplicationError::NoEmailAddress),
    }
}

/// A quantity as people write it in Brazil: a decimal comma, no trailing zeros.
fn quantity(value: Decimal) -> String {
    value.normalize().to_string().replace('.', ",")
}

/// An amount of money with a decimal comma: cents at least, and more decimals only when the amount has them (a unit
/// cost may).
fn money(value: Decimal) -> String {
    let value = value.normalize();
    let text = if value.scale() <= 2 {
        format!("{value:.2}")
    } else {
        value.to_string()
    };
    let (whole, cents) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let (sign, digits) = whole
        .strip_prefix('-')
        .map_or(("", whole), |digits| ("-", digits));
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(digit);
    }
    format!("{sign}{grouped},{cents}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_and_days_are_written_as_people_read_them_in_brazil() {
        assert_eq!(money(Decimal::new(1025, 2)), "10,25");
        assert_eq!(money(Decimal::from(10)), "10,00");
        assert_eq!(money(Decimal::new(333333, 6)), "0,333333");
        assert_eq!(money(Decimal::new(105, 1)), "10,50");
        assert_eq!(money(Decimal::new(123456789, 2)), "1.234.567,89");
        assert_eq!(money(Decimal::new(-100000, 2)), "-1.000,00");
        assert_eq!(quantity(Decimal::new(1500, 3)), "1,5");
        assert_eq!(quantity(Decimal::from(20)), "20");
    }
}
