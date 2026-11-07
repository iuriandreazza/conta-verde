use conta_verde_domain::{
    Actor, COOLDOWN_MILLIS, CalendarDate, CompanyId, DAILY_CAP, DAY_MILLIS, Decimal, DocumentMail,
    DocumentMailId, DocumentMailKind, Email, EntryId, EntryKind, Party, PartyId, PurchaseOrderId,
    PurchaseStatus, SettlementId,
};
use uuid::Uuid;

use crate::text::one_line;
use crate::{
    ApplicationError, DocumentMailRepository, EmailMessage, EmailSender, FinancialEntryRepository,
    PartyRepository, PurchaseOrderRepository, Reservation,
};

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
        message: EmailMessage,
    ) -> Result<Mailed, ApplicationError> {
        let Actor::User { id: sent_by, .. } = actor;
        let mail = DocumentMail {
            id: DocumentMailId::generate(),
            company_id,
            kind,
            document_id,
            to: message.to.clone(),
            sent_by: *sent_by,
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
        let emailed = self.emails.send(&message).await.is_ok();
        if !emailed {
            self.log.release(company_id, mail.id).await?;
        }
        Ok(Mailed {
            to: message.to,
            emailed,
        })
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
        let mut body = format!(
            "{} enviou o pedido de compra nº {}.\n\nItens:\n",
            one_line(company_name),
            order.number()
        );
        for line in order.lines() {
            body.push_str(&format!(
                "- {} x {}: R$ {} cada\n",
                quantity(line.quantity().as_decimal()),
                one_line(line.description()),
                money(line.unit_price().as_decimal())
            ));
        }
        body.push_str(&format!("\nTotal: R$ {}\n", money(order.total())));
        if let Some(day) = order.expected_on() {
            body.push_str(&format!("Entrega prevista: {}\n", br_date(day)));
        }
        if let Some(notes) = order.notes() {
            body.push_str(&format!("\nObservações: {}\n", one_line(notes)));
        }
        let message = EmailMessage {
            to: to.clone(),
            subject: format!(
                "Pedido de compra nº {} de {}",
                order.number(),
                one_line(company_name)
            ),
            body,
        };
        self.deliver(
            company_id,
            actor,
            now_millis,
            DocumentMailKind::PurchaseOrder,
            id.as_uuid(),
            message,
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
        let greeting = customer.as_ref().map_or_else(
            || "Olá.".to_owned(),
            |party| format!("Olá, {}.", one_line(party.name())),
        );
        let mut body = format!(
            "{greeting}\n\n{} confirma o recebimento do seu pagamento.\n\nReferente a: {}\nParcela: {} de {}\nData: {}\n",
            one_line(company_name),
            one_line(entry.description()),
            settlement.installment(),
            entry.installments().len(),
            br_date(settlement.settled_on()),
        );
        for (label, amount) in [
            ("Juros", settlement.interest().as_decimal()),
            ("Multa", settlement.fine().as_decimal()),
            ("Desconto", settlement.discount().as_decimal()),
        ] {
            if !amount.is_zero() {
                body.push_str(&format!("{label}: R$ {}\n", money(amount)));
            }
        }
        body.push_str(&format!("Valor pago: R$ {}\n", money(settlement.cash())));
        let message = EmailMessage {
            to: to.clone(),
            subject: format!("Comprovante de pagamento de {}", one_line(company_name)),
            body,
        };
        self.deliver(
            company_id,
            actor,
            now_millis,
            DocumentMailKind::PaymentReceipt,
            settlement_id.as_uuid(),
            message,
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

fn br_date(day: CalendarDate) -> String {
    format!("{:02}/{:02}/{}", day.day(), day.month(), day.year())
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
        assert_eq!(
            br_date(CalendarDate::parse("2026-11-06").unwrap()),
            "06/11/2026"
        );
    }
}
