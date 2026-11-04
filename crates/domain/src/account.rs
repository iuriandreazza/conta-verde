use crate::{CompanyId, DomainError, Email, UserId};

const MIN_PASSWORD_CHARS: usize = 8;
const MAX_PASSWORD_CHARS: usize = 128;

/// Unvalidated input for creating a user together with its first company.
#[derive(Debug, Clone)]
pub struct AccountDraft {
    pub user_name: String,
    pub email: String,
    pub company_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    id: UserId,
    name: String,
    email: Email,
}

impl User {
    /// Rebuilds a user read from storage, where the data was validated when written.
    pub fn restore(id: UserId, name: String, email: Email) -> Self {
        Self { id, name, email }
    }

    pub fn id(&self) -> UserId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &Email {
        &self.email
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Company {
    id: CompanyId,
    name: String,
}

impl Company {
    /// Rebuilds a company read from storage, where the data was validated when written.
    pub fn restore(id: CompanyId, name: String) -> Self {
        Self { id, name }
    }

    pub fn id(&self) -> CompanyId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A user and the company they own, created together so nobody is ever left without a company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub user: User,
    pub company: Company,
}

impl Account {
    pub fn new(
        user_id: UserId,
        company_id: CompanyId,
        draft: AccountDraft,
    ) -> Result<Self, DomainError> {
        let user_name = non_empty(&draft.user_name)?;
        let company_name = non_empty(&draft.company_name)?;
        let email = Email::parse(&draft.email)?;
        Ok(Self {
            user: User::restore(user_id, user_name, email),
            company: Company::restore(company_id, company_name),
        })
    }
}

pub fn validate_password(password: &str) -> Result<(), DomainError> {
    let length = password.chars().count();
    if (MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&length) {
        Ok(())
    } else {
        Err(DomainError::WeakPassword)
    }
}

fn non_empty(raw: &str) -> Result<String, DomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Err(DomainError::EmptyName)
    } else {
        Ok(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> AccountDraft {
        AccountDraft {
            user_name: " Ana ".into(),
            email: "Ana@Example.com".into(),
            company_name: "Padaria Central".into(),
        }
    }

    fn build(draft: AccountDraft) -> Result<Account, DomainError> {
        Account::new(UserId::generate(), CompanyId::generate(), draft)
    }

    #[test]
    fn builds_user_and_company_with_normalized_fields() {
        let account = build(draft()).unwrap();
        assert_eq!(account.user.name(), "Ana");
        assert_eq!(account.user.email().as_str(), "ana@example.com");
        assert_eq!(account.company.name(), "Padaria Central");
    }

    #[test]
    fn rejects_blank_names_and_bad_email() {
        let blank_company = AccountDraft {
            company_name: " ".into(),
            ..draft()
        };
        assert_eq!(build(blank_company), Err(DomainError::EmptyName));
        let bad_email = AccountDraft {
            email: "ana".into(),
            ..draft()
        };
        assert_eq!(build(bad_email), Err(DomainError::InvalidEmail));
    }

    #[test]
    fn password_length_is_bounded() {
        assert_eq!(validate_password("short"), Err(DomainError::WeakPassword));
        assert_eq!(
            validate_password(&"x".repeat(129)),
            Err(DomainError::WeakPassword)
        );
        assert!(validate_password("long enough").is_ok());
    }
}
