use conta_verde_domain::{
    CalendarDate, CompanyId, Decimal, Email, EntryId, EntryKind, Party, PartyId, PurchaseOrderId,
    PurchaseStatus, SettlementId,
};

use crate::text::one_line;
use crate::{
    ApplicationError, EmailMessage, EmailSender, FinancialEntryRepository, PartyRepository,
    PurchaseOrderRepository,
};

/// An e-mail that was asked for: to whom, and whether it was taken for sending (queued, [ADR 0059]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailed {
    pub to: Email,
    pub emailed: bool,
}

/// Sends a person a document as text: a purchase order to its supplier, the proof of a payment to the customer who
/// made it ([ADR 0061]). It writes nothing, so there is no audit entry: a message that is not taken is `emailed: false`,
/// and the person asking is told.
pub struct DocumentMailService<P, E, Y, M> {
    purchases: P,
    entries: E,
    parties: Y,
    emails: M,
}

impl<P, E, Y, M> DocumentMailService<P, E, Y, M>
where
    P: PurchaseOrderRepository,
    E: FinancialEntryRepository,
    Y: PartyRepository,
    M: EmailSender,
{
    pub fn new(purchases: P, entries: E, parties: Y, emails: M) -> Self {
        Self {
            purchases,
            entries,
            parties,
            emails,
        }
    }

    /// E-mails an approved or partly received purchase order to `to`, or to its supplier's address.
    pub async fn send_purchase_order(
        &self,
        company_id: CompanyId,
        id: PurchaseOrderId,
        to: Option<&str>,
        company_name: &str,
    ) -> Result<Mailed, ApplicationError> {
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
        Ok(Mailed {
            to,
            emailed: self.emails.send(&message).await.is_ok(),
        })
    }

    /// E-mails the customer the proof of one settlement of a receivable that was not reversed.
    pub async fn send_payment_receipt(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        settlement_id: SettlementId,
        to: Option<&str>,
        company_name: &str,
    ) -> Result<Mailed, ApplicationError> {
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
        Ok(Mailed {
            to,
            emailed: self.emails.send(&message).await.is_ok(),
        })
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
