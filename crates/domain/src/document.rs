use crate::DomainError;

const CPF_LEN: usize = 11;
const CNPJ_LEN: usize = 14;
const CPF_WEIGHTS: [u32; 10] = [11, 10, 9, 8, 7, 6, 5, 4, 3, 2];
const CNPJ_WEIGHTS: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];

/// Brazilian taxpayer registry number, stored as digits only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Document {
    Cpf(String),
    Cnpj(String),
}

impl Document {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let digits = raw
            .chars()
            .filter(|c| !matches!(c, '.' | '-' | '/') && !c.is_whitespace())
            .map(|c| c.to_digit(10).ok_or(DomainError::InvalidDocument))
            .collect::<Result<Vec<_>, _>>()?;

        // A repeated digit (000.000.000-00) passes the checksum but is not issued.
        if digits.windows(2).all(|pair| pair[0] == pair[1]) {
            return Err(DomainError::InvalidDocument);
        }

        let text: String = digits
            .iter()
            .filter_map(|d| char::from_digit(*d, 10))
            .collect();
        match digits.len() {
            CPF_LEN if has_valid_check_digits(&digits, &CPF_WEIGHTS) => Ok(Self::Cpf(text)),
            CNPJ_LEN if has_valid_check_digits(&digits, &CNPJ_WEIGHTS) => Ok(Self::Cnpj(text)),
            _ => Err(DomainError::InvalidDocument),
        }
    }

    pub fn digits(&self) -> &str {
        match self {
            Self::Cpf(digits) | Self::Cnpj(digits) => digits,
        }
    }
}

/// Both check digits use the same modulo-11 rule; only the weights differ.
/// Each digit is weighted with the tail of `weights` that matches its position.
fn has_valid_check_digits(digits: &[u32], weights: &[u32]) -> bool {
    let body_len = digits.len() - 2;
    (body_len..digits.len()).all(|position| {
        let tail = &weights[weights.len() - position..];
        check_digit(&digits[..position], tail) == digits[position]
    })
}

fn check_digit(digits: &[u32], weights: &[u32]) -> u32 {
    let sum: u32 = digits.iter().zip(weights).map(|(d, w)| d * w).sum();
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
    fn accepts_formatted_cnpj() {
        let document = Document::parse("11.222.333/0001-81").unwrap();
        assert_eq!(document, Document::Cnpj("11222333000181".into()));
    }

    #[test]
    fn rejects_wrong_check_digit() {
        assert_eq!(
            Document::parse("529.982.247-24"),
            Err(DomainError::InvalidDocument)
        );
        assert_eq!(
            Document::parse("11.222.333/0001-82"),
            Err(DomainError::InvalidDocument)
        );
    }

    #[test]
    fn rejects_repeated_digits_and_bad_lengths_and_letters() {
        assert!(Document::parse("111.111.111-11").is_err());
        assert!(Document::parse("123").is_err());
        assert!(Document::parse("5299822472a").is_err());
    }
}
