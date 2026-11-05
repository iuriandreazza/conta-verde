use crate::DomainError;

const CPF_LEN: usize = 11;
const CNPJ_LEN: usize = 14;
const CPF_WEIGHTS: [u32; 10] = [11, 10, 9, 8, 7, 6, 5, 4, 3, 2];
const CNPJ_WEIGHTS: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
// Receita Federal maps every character to its ASCII code minus 48, so '0'..'9' keep their value
// and 'A'..'Z' become 17..42 (alphanumeric CNPJ, issued since July 2026).
const ASCII_OFFSET: u32 = 48;

/// Brazilian taxpayer registry number, stored without punctuation. A CPF is 11 digits; a CNPJ is
/// 14 characters whose first 12 may be digits or uppercase letters and whose last two are digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Document {
    Cpf(String),
    Cnpj(String),
}

impl Document {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let text: String = raw
            .chars()
            .filter(|c| !matches!(c, '.' | '-' | '/') && !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let values = text
            .chars()
            .map(|c| {
                (c.is_ascii_digit() || c.is_ascii_uppercase())
                    .then(|| c as u32 - ASCII_OFFSET)
                    .ok_or(DomainError::InvalidDocument)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // A repeated character (000.000.000-00) passes the checksum but is not issued.
        if values.windows(2).all(|pair| pair[0] == pair[1]) {
            return Err(DomainError::InvalidDocument);
        }

        // Only the CNPJ body can hold letters, and a letter in a check-digit slot never equals the
        // computed digit (0 to 9), so the checksum comparison rejects it.
        let is_numeric = values.iter().all(|value| *value < 10);
        match values.len() {
            CPF_LEN if is_numeric && has_valid_check_digits(&values, &CPF_WEIGHTS) => {
                Ok(Self::Cpf(text))
            }
            CNPJ_LEN if has_valid_check_digits(&values, &CNPJ_WEIGHTS) => Ok(Self::Cnpj(text)),
            _ => Err(DomainError::InvalidDocument),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Cpf(text) | Self::Cnpj(text) => text,
        }
    }

    /// The CNPJ, when this is one. Only companies exist in the federal registry we can look up.
    pub fn cnpj(&self) -> Option<&str> {
        match self {
            Self::Cnpj(text) => Some(text),
            Self::Cpf(_) => None,
        }
    }
}

/// Both check digits use the same modulo-11 rule; only the weights differ.
/// Each character is weighted with the tail of `weights` that matches its position.
fn has_valid_check_digits(values: &[u32], weights: &[u32]) -> bool {
    let body_len = values.len() - 2;
    (body_len..values.len()).all(|position| {
        let tail = &weights[weights.len() - position..];
        check_digit(&values[..position], tail) == values[position]
    })
}

fn check_digit(values: &[u32], weights: &[u32]) -> u32 {
    let sum: u32 = values.iter().zip(weights).map(|(v, w)| v * w).sum();
    match sum % 11 {
        0 | 1 => 0,
        remainder => 11 - remainder,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_formatted_cpf() {
        let document = Document::parse("529.982.247-25").unwrap();
        assert_eq!(document, Document::Cpf("52998224725".into()));
    }

    #[test]
    fn accepts_formatted_numeric_cnpj() {
        let document = Document::parse("11.222.333/0001-81").unwrap();
        assert_eq!(document, Document::Cnpj("11222333000181".into()));
    }

    // Examples from Receita Federal's "CNPJ alfanumérico: perguntas e respostas" (questions 14, 17
    // and 23). That document prints "AA345678/0003-29" as an illustration, but its own algorithm
    // gives -86 for that number (computed by hand), so the case below uses the corrected digits.
    #[test]
    fn accepts_the_official_alphanumeric_cnpj_examples() {
        for (formatted, stored) in [
            ("12.ABC.345/01DE-35", "12ABC34501DE35"),
            ("AA345678/0003-86", "AA345678000386"),
            ("AA345678/000A-29", "AA345678000A29"),
            ("12.345.678/000A-08", "12345678000A08"),
            ("12.345.678/0001-95", "12345678000195"),
        ] {
            assert_eq!(
                Document::parse(formatted),
                Ok(Document::Cnpj(stored.into())),
                "{formatted}"
            );
        }
    }

    #[test]
    fn normalizes_lowercase_letters() {
        assert_eq!(
            Document::parse("12.abc.345/01de-35"),
            Ok(Document::Cnpj("12ABC34501DE35".into()))
        );
    }

    #[test]
    fn rejects_wrong_check_digit() {
        for raw in ["529.982.247-24", "11.222.333/0001-82", "12.ABC.345/01DE-36"] {
            assert_eq!(
                Document::parse(raw),
                Err(DomainError::InvalidDocument),
                "{raw}"
            );
        }
    }

    #[test]
    fn letters_are_only_allowed_in_the_cnpj_body() {
        // CPF is digits only, and a CNPJ check digit can never be a letter.
        assert!(Document::parse("5299822472a").is_err());
        assert!(Document::parse("12.ABC.345/01DE-3A").is_err());
        assert!(Document::parse("12.ABC.345/01DE-A5").is_err());
    }

    #[test]
    fn rejects_repeated_characters_bad_lengths_and_symbols() {
        for raw in [
            "111.111.111-11",
            "00000000000000",
            "123",
            "12.ABC.345/01DE-3",
            "12.ABC.345/01D*-35",
        ] {
            assert!(Document::parse(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn only_a_cnpj_exposes_a_registry_number() {
        let cpf = Document::parse("529.982.247-25").unwrap();
        let cnpj = Document::parse("11.222.333/0001-81").unwrap();
        assert_eq!(cpf.cnpj(), None);
        assert_eq!(cnpj.cnpj(), Some("11222333000181"));
    }
}
