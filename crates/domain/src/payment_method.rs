use crate::name::clean_name;
use crate::{CompanyId, DomainError, PaymentMethodId};

/// How a payment is made. The keys are stored, in a CHECK and in the audit trail, so they do not
/// change; the labels live in the web app. The codes a fiscal document asks for (the NF-e's) are
/// not here: they come from the official table, with the fiscal phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentMethodKind {
    Cash,
    Pix,
    BankSlip,
    CreditCard,
    DebitCard,
    BankTransfer,
    Check,
    Other,
}

impl PaymentMethodKind {
    const ALL: [Self; 8] = [
        Self::Cash,
        Self::Pix,
        Self::BankSlip,
        Self::CreditCard,
        Self::DebitCard,
        Self::BankTransfer,
        Self::Check,
        Self::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::Pix => "pix",
            Self::BankSlip => "bank_slip",
            Self::CreditCard => "credit_card",
            Self::DebitCard => "debit_card",
            Self::BankTransfer => "bank_transfer",
            Self::Check => "check",
            Self::Other => "other",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }
}

/// Unvalidated input for registering or changing a payment method.
#[derive(Debug, Clone)]
pub struct PaymentMethodDraft {
    pub kind: PaymentMethodKind,
    pub name: String,
    pub active: bool,
}

/// A way of paying or being paid that a company accepts, such as "Pix" or "Cartão Visa". What it is
/// (the kind) is what reports and the fiscal phase read; the name is what people see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentMethod {
    id: PaymentMethodId,
    company_id: CompanyId,
    kind: PaymentMethodKind,
    name: String,
    active: bool,
}

impl PaymentMethod {
    pub fn new(
        id: PaymentMethodId,
        company_id: CompanyId,
        draft: PaymentMethodDraft,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            company_id,
            kind: draft.kind,
            name: clean_name(&draft.name)?,
            active: draft.active,
        })
    }

    /// The same payment method with every editable field replaced by the draft's.
    pub fn replaced_with(&self, draft: PaymentMethodDraft) -> Result<Self, DomainError> {
        Self::new(self.id, self.company_id, draft)
    }

    pub fn id(&self) -> PaymentMethodId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn kind(&self) -> PaymentMethodKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(name: &str, kind: PaymentMethodKind) -> Result<PaymentMethod, DomainError> {
        PaymentMethod::new(
            PaymentMethodId::generate(),
            CompanyId::generate(),
            PaymentMethodDraft {
                kind,
                name: name.into(),
                active: true,
            },
        )
    }

    #[test]
    fn keeps_a_trimmed_name_and_refuses_a_blank_or_unstorable_one() {
        assert_eq!(
            build("  Pix ", PaymentMethodKind::Pix).unwrap().name(),
            "Pix"
        );
        assert_eq!(
            build(" ", PaymentMethodKind::Pix).unwrap_err(),
            DomainError::EmptyName
        );
        assert_eq!(
            build("P\0ix", PaymentMethodKind::Pix).unwrap_err(),
            DomainError::InvalidName
        );
    }

    #[test]
    fn the_kind_may_change_with_the_rest() {
        let method = build("Visa", PaymentMethodKind::CreditCard).unwrap();
        let replaced = method
            .replaced_with(PaymentMethodDraft {
                kind: PaymentMethodKind::DebitCard,
                name: "Visa débito".into(),
                active: false,
            })
            .unwrap();
        assert_eq!(replaced.id(), method.id());
        assert_eq!(replaced.kind(), PaymentMethodKind::DebitCard);
        assert!(!replaced.is_active());
    }

    #[test]
    fn the_kind_keys_are_stable_and_round_trip() {
        for key in [
            "cash",
            "pix",
            "bank_slip",
            "credit_card",
            "debit_card",
            "bank_transfer",
            "check",
            "other",
        ] {
            assert_eq!(PaymentMethodKind::parse(key).unwrap().as_str(), key);
        }
        assert_eq!(PaymentMethodKind::parse("boleto"), None);
    }
}
