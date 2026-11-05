use crate::name::{clean_name, non_blank};
use crate::{BankAccountId, CompanyId, DomainError};

const MAX_BRANCH_LEN: usize = 10;
const MAX_NUMBER_LEN: usize = 20;

/// What kind of account holds the money. Fixed when the account is registered: what is asked of it
/// (a bank, a branch, a number) and what Phase 2 does with it (bills for a card) depend on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankAccountKind {
    Checking,
    Savings,
    /// Cash on hand: no bank.
    Cash,
    /// A credit card, whose bills are paid from another account. Only its issuing bank is kept;
    /// never a card number.
    Card,
}

impl BankAccountKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Checking => "checking",
            Self::Savings => "savings",
            Self::Cash => "cash",
            Self::Card => "card",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [Self::Checking, Self::Savings, Self::Cash, Self::Card]
            .into_iter()
            .find(|kind| kind.as_str() == raw)
    }
}

/// Unvalidated input for registering or changing a bank account.
#[derive(Debug, Clone)]
pub struct BankAccountDraft {
    pub kind: BankAccountKind,
    pub name: String,
    /// The institution: its 3-digit code in the banking system (COMPE), or its 8-digit ISPB for
    /// those that have none.
    pub bank_code: Option<String>,
    pub branch: Option<String>,
    pub number: Option<String>,
    pub active: bool,
}

/// An account of a company: where money is kept and moves between. It holds no balance: opening
/// balances and movements come with the entries (Phase 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankAccount {
    id: BankAccountId,
    company_id: CompanyId,
    kind: BankAccountKind,
    name: String,
    bank_code: Option<String>,
    branch: Option<String>,
    number: Option<String>,
    active: bool,
}

impl BankAccount {
    pub fn new(
        id: BankAccountId,
        company_id: CompanyId,
        draft: BankAccountDraft,
    ) -> Result<Self, DomainError> {
        let bank_code = draft
            .bank_code
            .as_deref()
            .and_then(non_blank)
            .map(clean_bank_code)
            .transpose()?;
        let branch = draft
            .branch
            .as_deref()
            .and_then(non_blank)
            .map(|raw| clean_identifier(raw, MAX_BRANCH_LEN, DomainError::InvalidBranch))
            .transpose()?;
        let number = draft
            .number
            .as_deref()
            .and_then(non_blank)
            .map(|raw| clean_identifier(raw, MAX_NUMBER_LEN, DomainError::InvalidAccountNumber))
            .transpose()?;
        // Cash is not in a bank; refusing is clearer than storing a value that would never be used.
        if draft.kind == BankAccountKind::Cash
            && (bank_code.is_some() || branch.is_some() || number.is_some())
        {
            return Err(DomainError::CashAccountHasNoBank);
        }
        // A card is issued by a bank but has no branch or account number, and a card number must
        // not be stored at all: the audit trail is append-only and would keep it for good.
        if draft.kind == BankAccountKind::Card && (branch.is_some() || number.is_some()) {
            return Err(DomainError::CardAccountHasNoNumber);
        }
        Ok(Self {
            id,
            company_id,
            kind: draft.kind,
            name: clean_name(&draft.name)?,
            bank_code,
            branch,
            number,
            active: draft.active,
        })
    }

    /// The same account with every editable field replaced by the draft's, validated as for a new
    /// one. The kind is fixed when it is registered.
    pub fn replaced_with(&self, draft: BankAccountDraft) -> Result<Self, DomainError> {
        if draft.kind != self.kind {
            return Err(DomainError::KindCannotChange);
        }
        Self::new(self.id, self.company_id, draft)
    }

    pub fn id(&self) -> BankAccountId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn kind(&self) -> BankAccountKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bank_code(&self) -> Option<&str> {
        self.bank_code.as_deref()
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    pub fn number(&self) -> Option<&str> {
        self.number.as_deref()
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// Three digits, or eight when the institution has no such code.
fn clean_bank_code(raw: &str) -> Result<String, DomainError> {
    let digits = raw.bytes().all(|byte| byte.is_ascii_digit());
    if digits && (raw.len() == 3 || raw.len() == 8) {
        Ok(raw.to_owned())
    } else {
        Err(DomainError::InvalidBankCode)
    }
}

/// A branch or an account number in one form: upper case letters and digits, without the spaces,
/// dots and hyphens people put in (`1234-5` is `12345`), the check digit kept. One form is what
/// makes "the same account" something a unique index and, later, an OFX file can recognise.
fn clean_identifier(raw: &str, max_len: usize, error: DomainError) -> Result<String, DomainError> {
    let identifier: String = raw
        .chars()
        .filter(|c| !matches!(c, '.' | '-') && !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let valid = !identifier.is_empty()
        && identifier.len() <= max_len
        && identifier
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if valid { Ok(identifier) } else { Err(error) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(kind: BankAccountKind) -> BankAccountDraft {
        BankAccountDraft {
            kind,
            name: "  Conta principal ".into(),
            bank_code: Some("341".into()),
            branch: Some(" 1234 ".into()),
            number: Some("56789-0".into()),
            active: true,
        }
    }

    fn build(draft: BankAccountDraft) -> Result<BankAccount, DomainError> {
        BankAccount::new(BankAccountId::generate(), CompanyId::generate(), draft)
    }

    #[test]
    fn keeps_one_form_of_the_branch_and_the_number() {
        let account = build(BankAccountDraft {
            branch: Some("12.34 ".into()),
            number: Some("56789-x".into()),
            ..draft(BankAccountKind::Checking)
        })
        .unwrap();
        assert_eq!(account.name(), "Conta principal");
        assert_eq!(account.branch(), Some("1234"));
        assert_eq!(account.number(), Some("56789X"));
        assert_eq!(account.bank_code(), Some("341"));
    }

    #[test]
    fn blank_optional_fields_are_not_set() {
        let account = build(BankAccountDraft {
            bank_code: Some(" ".into()),
            branch: Some("".into()),
            number: None,
            ..draft(BankAccountKind::Savings)
        })
        .unwrap();
        assert_eq!(
            (account.bank_code(), account.branch(), account.number()),
            (None, None, None)
        );
    }

    #[test]
    fn a_bank_code_is_three_digits_or_eight() {
        let with = |code: &str| {
            build(BankAccountDraft {
                bank_code: Some(code.into()),
                ..draft(BankAccountKind::Checking)
            })
        };
        assert!(with("001").is_ok());
        assert!(with("00000000").is_ok());
        for bad in ["01", "0001", "1234567", "123456789", "34a", "٣٤١", "-34"] {
            assert_eq!(
                with(bad).unwrap_err(),
                DomainError::InvalidBankCode,
                "{bad}"
            );
        }
    }

    #[test]
    fn rejects_a_branch_or_number_that_is_not_letters_and_digits() {
        let branch = |value: &str| {
            build(BankAccountDraft {
                branch: Some(value.into()),
                ..draft(BankAccountKind::Checking)
            })
        };
        assert_eq!(branch("12 3/4").unwrap_err(), DomainError::InvalidBranch);
        assert_eq!(branch("ç").unwrap_err(), DomainError::InvalidBranch);
        assert_eq!(
            branch("12345678901").unwrap_err(),
            DomainError::InvalidBranch
        );
        assert_eq!(branch("--").unwrap_err(), DomainError::InvalidBranch);
        let number = |value: &str| {
            build(BankAccountDraft {
                number: Some(value.into()),
                ..draft(BankAccountKind::Checking)
            })
        };
        assert_eq!(
            number("1_2").unwrap_err(),
            DomainError::InvalidAccountNumber
        );
        assert_eq!(
            number(&"1".repeat(21)).unwrap_err(),
            DomainError::InvalidAccountNumber
        );
    }

    #[test]
    fn cash_has_no_bank_and_a_card_has_no_number() {
        assert_eq!(
            build(draft(BankAccountKind::Cash)).unwrap_err(),
            DomainError::CashAccountHasNoBank
        );
        let cash = BankAccountDraft {
            bank_code: None,
            branch: None,
            number: None,
            ..draft(BankAccountKind::Cash)
        };
        assert!(build(cash).is_ok());
        // A card keeps the issuer, and refuses anything that could be (part of) a card number.
        let card = |branch: Option<&str>, number: Option<&str>| {
            build(BankAccountDraft {
                branch: branch.map(Into::into),
                number: number.map(Into::into),
                ..draft(BankAccountKind::Card)
            })
        };
        assert!(card(None, None).is_ok());
        assert_eq!(
            card(None, Some("4111111111111111")).unwrap_err(),
            DomainError::CardAccountHasNoNumber
        );
        assert_eq!(
            card(Some("1"), None).unwrap_err(),
            DomainError::CardAccountHasNoNumber
        );
        for kind in [BankAccountKind::Checking, BankAccountKind::Savings] {
            assert!(build(draft(kind)).is_ok());
        }
        // Nothing is required: an account can be registered by its name alone.
        assert!(
            build(BankAccountDraft {
                bank_code: None,
                branch: None,
                number: None,
                ..draft(BankAccountKind::Checking)
            })
            .is_ok()
        );
    }

    #[test]
    fn replacing_keeps_the_identity_and_refuses_a_change_of_kind() {
        let account = build(draft(BankAccountKind::Checking)).unwrap();
        let replaced = account
            .replaced_with(BankAccountDraft {
                name: "Conta da loja".into(),
                active: false,
                ..draft(BankAccountKind::Checking)
            })
            .unwrap();
        assert_eq!(replaced.id(), account.id());
        assert_eq!(replaced.company_id(), account.company_id());
        assert_eq!(replaced.name(), "Conta da loja");
        assert!(!replaced.is_active());
        assert_eq!(
            account
                .replaced_with(draft(BankAccountKind::Savings))
                .unwrap_err(),
            DomainError::KindCannotChange
        );
    }

    #[test]
    fn the_kind_keys_are_stable_and_round_trip() {
        for key in ["checking", "savings", "cash", "card"] {
            assert_eq!(BankAccountKind::parse(key).unwrap().as_str(), key);
        }
        assert_eq!(BankAccountKind::parse("Checking"), None);
    }
}
