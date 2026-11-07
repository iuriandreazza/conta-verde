use uuid::Uuid;

use crate::{CompanyId, DocumentMailId, Email, UserId};

/// How long the same document is not sent again to the same address: a double click, or a person who did not see that it
/// went, does not send it twice.
pub const COOLDOWN_MILLIS: i64 = 5 * 60 * 1000;

/// How many documents a company may have e-mailed over a day, whoever sends them: the e-mail leaves from the company's
/// sender address, and this keeps a stolen session or a loop from burning its reputation.
pub const DAILY_CAP: u32 = 100;

/// The length of the day the cap is counted over.
pub const DAY_MILLIS: i64 = 24 * 60 * 60 * 1000;

/// A kind of document that is e-mailed on request ([ADR 0061]). The key is stored, so it does not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentMailKind {
    PurchaseOrder,
    PaymentReceipt,
}

impl DocumentMailKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PurchaseOrder => "purchase_order",
            Self::PaymentReceipt => "payment_receipt",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "purchase_order" => Some(Self::PurchaseOrder),
            "payment_receipt" => Some(Self::PaymentReceipt),
            _ => None,
        }
    }
}

/// That a document was e-mailed: which, to whom, by whom and when. It is a log, never edited or deleted (except to take
/// back a send that went nowhere), and it is not audited: the document did not change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentMail {
    pub id: DocumentMailId,
    pub company_id: CompanyId,
    pub kind: DocumentMailKind,
    /// The purchase order, or the settlement a receipt proves.
    pub document_id: Uuid,
    pub to: Email,
    pub sent_by: UserId,
    /// Unix milliseconds.
    pub sent_at_millis: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_round_trip() {
        for kind in [
            DocumentMailKind::PurchaseOrder,
            DocumentMailKind::PaymentReceipt,
        ] {
            assert_eq!(DocumentMailKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(DocumentMailKind::parse("quote"), None);
    }
}
