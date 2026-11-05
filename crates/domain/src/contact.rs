use crate::name::{optional_text, required_text};
use crate::{DomainError, Email};

pub const MAX_CONTACTS: usize = 20;
const MAX_NAME_CHARS: usize = 100;
const MAX_JOB_TITLE_CHARS: usize = 80;
const MAX_PHONE_CHARS: usize = 30;
const MIN_PHONE_DIGITS: usize = 8;
const MAX_PHONE_DIGITS: usize = 15;

/// Unvalidated input for a contact: what a form sends.
#[derive(Debug, Clone, Default)]
pub struct ContactDraft {
    pub name: String,
    pub job_title: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
}

/// A person to talk to at a party. Only the name is required; the phone is kept as written (a
/// company may write `(81) 99999-0000` or `+55 81 9999-0000`) once its shape is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    name: String,
    job_title: Option<String>,
    email: Option<Email>,
    phone: Option<String>,
}

impl Contact {
    pub fn new(draft: ContactDraft) -> Result<Self, DomainError> {
        Ok(Self {
            name: required_text(&draft.name, MAX_NAME_CHARS, invalid("name"))?,
            job_title: optional_text(
                draft.job_title.as_deref(),
                MAX_JOB_TITLE_CHARS,
                invalid("job title"),
            )?,
            email: draft.email.as_deref().map(Email::parse).transpose()?,
            phone: draft.phone.as_deref().map(clean_phone).transpose()?,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn job_title(&self) -> Option<&str> {
        self.job_title.as_deref()
    }

    pub fn email(&self) -> Option<&Email> {
        self.email.as_ref()
    }

    pub fn phone(&self) -> Option<&str> {
        self.phone.as_deref()
    }
}

fn invalid(field: &'static str) -> DomainError {
    DomainError::InvalidContactField(field)
}

/// Digits, spaces and the punctuation phone numbers are written with, holding 8 to 15 digits (the
/// longest international number has 15). Whether the number exists is not something to check.
fn clean_phone(raw: &str) -> Result<String, DomainError> {
    let phone = raw.trim();
    let digits = phone.chars().filter(char::is_ascii_digit).count();
    let written_like_a_phone = phone
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '+' | '-' | '(' | ')' | '.'));
    if written_like_a_phone
        && phone.chars().count() <= MAX_PHONE_CHARS
        && (MIN_PHONE_DIGITS..=MAX_PHONE_DIGITS).contains(&digits)
    {
        Ok(phone.to_owned())
    } else {
        Err(DomainError::InvalidPhone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> ContactDraft {
        ContactDraft {
            name: "  Ana Souza ".into(),
            job_title: Some(" Compras ".into()),
            email: Some(" Ana@Padaria.example ".into()),
            phone: Some(" (81) 99999-0000 ".into()),
        }
    }

    #[test]
    fn trims_and_keeps_the_phone_as_written() {
        let contact = Contact::new(draft()).unwrap();
        assert_eq!(contact.name(), "Ana Souza");
        assert_eq!(contact.job_title(), Some("Compras"));
        assert_eq!(contact.email().unwrap().as_str(), "ana@padaria.example");
        assert_eq!(contact.phone(), Some("(81) 99999-0000"));
    }

    #[test]
    fn only_the_name_is_required_and_blank_text_is_not_set() {
        let contact = Contact::new(ContactDraft {
            name: "Ana".into(),
            job_title: Some("  ".into()),
            ..ContactDraft::default()
        })
        .unwrap();
        assert_eq!(
            (contact.job_title(), contact.email(), contact.phone()),
            (None, None, None)
        );
    }

    #[test]
    fn the_name_is_required_and_bounded_and_the_error_names_the_field() {
        for name in ["", "  ", "a\0b"] {
            let result = Contact::new(ContactDraft {
                name: name.into(),
                ..draft()
            });
            assert_eq!(result, Err(DomainError::InvalidContactField("name")));
        }
        let long_title = Contact::new(ContactDraft {
            job_title: Some("a".repeat(81)),
            ..draft()
        });
        assert_eq!(
            long_title,
            Err(DomainError::InvalidContactField("job title"))
        );
    }

    #[test]
    fn a_malformed_email_is_refused() {
        let result = Contact::new(ContactDraft {
            email: Some("ana".into()),
            ..draft()
        });
        assert_eq!(result, Err(DomainError::InvalidEmail));
    }

    #[test]
    fn a_phone_is_written_like_one() {
        for raw in [
            "(81) 99999-0000",
            "+55 81 9999-0000",
            "81.9999.0000",
            "1234 5678",
        ] {
            assert!(
                Contact::new(ContactDraft {
                    phone: Some(raw.into()),
                    ..draft()
                })
                .is_ok(),
                "{raw}"
            );
        }
        for raw in [
            "",
            "123 456",
            "ligar depois",
            "81 99999-0000 ramal 12",
            "1234567890123456",
            "+55 (81) 99999-0000 0000 0000 000",
        ] {
            let result = Contact::new(ContactDraft {
                phone: Some(raw.into()),
                ..draft()
            });
            assert_eq!(result, Err(DomainError::InvalidPhone), "{raw}");
        }
    }
}
