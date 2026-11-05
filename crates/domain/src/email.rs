use crate::DomainError;

/// A syntactically plausible email address, trimmed and lowercased so equality and uniqueness ignore case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let email = raw.trim();
        // A NUL byte is one Postgres refuses to store, and a line break would add headers to a
        // message sent to the address: every control character is refused here, as a 422.
        match email.split_once('@') {
            Some((local, domain))
                if !local.is_empty()
                    && domain.contains('.')
                    && !email.chars().any(char::is_control) =>
            {
                Ok(Self(email.to_lowercase()))
            }
            _ => Err(DomainError::InvalidEmail),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_whitespace() {
        let email = Email::parse("  Ana@Example.COM ").unwrap();
        assert_eq!(email.as_str(), "ana@example.com");
    }

    #[test]
    fn rejects_malformed_addresses() {
        for raw in [
            "",
            "ana",
            "@example.com",
            "ana@example",
            "a\0@example.com",
            "ana@example.com\r\nBcc: x@example.com",
            "a\nb@example.com",
        ] {
            assert_eq!(Email::parse(raw), Err(DomainError::InvalidEmail), "{raw}");
        }
    }
}
