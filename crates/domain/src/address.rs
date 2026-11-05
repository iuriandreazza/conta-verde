use crate::DomainError;
use crate::name::{optional_text, required_text};

const POSTAL_CODE_LEN: usize = 8;
const MAX_STREET_CHARS: usize = 150;
const MAX_NUMBER_CHARS: usize = 20;
const MAX_COMPLEMENT_CHARS: usize = 80;
const MAX_DISTRICT_CHARS: usize = 80;
const MAX_CITY_CHARS: usize = 80;

/// The 26 states and the Federal District, by their two-letter abbreviation.
const STATES: [&str; 27] = [
    "AC", "AL", "AM", "AP", "BA", "CE", "DF", "ES", "GO", "MA", "MG", "MS", "MT", "PA", "PB", "PE",
    "PI", "PR", "RJ", "RN", "RO", "RR", "RS", "SC", "SE", "SP", "TO",
];

/// Unvalidated input for an address: what a form sends.
#[derive(Debug, Clone, Default)]
pub struct AddressDraft {
    pub postal_code: String,
    pub street: String,
    pub number: Option<String>,
    pub complement: Option<String>,
    pub district: Option<String>,
    pub city: String,
    pub state: String,
}

/// A Brazilian postal address. The CEP, the street, the city and the state are always there; the
/// number (it may be "S/N" or missing), the complement and the district are optional. Only the
/// shape is checked: whether a CEP exists, or belongs to that street, is a lookup, not a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    postal_code: String,
    street: String,
    number: Option<String>,
    complement: Option<String>,
    district: Option<String>,
    city: String,
    state: String,
}

impl Address {
    pub fn new(draft: AddressDraft) -> Result<Self, DomainError> {
        Ok(Self {
            postal_code: PostalCode::parse(&draft.postal_code)?.0,
            street: required_text(&draft.street, MAX_STREET_CHARS, invalid("street"))?,
            number: optional_text(draft.number.as_deref(), MAX_NUMBER_CHARS, invalid("number"))?,
            complement: optional_text(
                draft.complement.as_deref(),
                MAX_COMPLEMENT_CHARS,
                invalid("complement"),
            )?,
            district: optional_text(
                draft.district.as_deref(),
                MAX_DISTRICT_CHARS,
                invalid("district"),
            )?,
            city: required_text(&draft.city, MAX_CITY_CHARS, invalid("city"))?,
            state: clean_state(&draft.state)?,
        })
    }

    /// Eight digits, without the hyphen.
    pub fn postal_code(&self) -> &str {
        &self.postal_code
    }

    pub fn street(&self) -> &str {
        &self.street
    }

    pub fn number(&self) -> Option<&str> {
        self.number.as_deref()
    }

    pub fn complement(&self) -> Option<&str> {
        self.complement.as_deref()
    }

    pub fn district(&self) -> Option<&str> {
        self.district.as_deref()
    }

    pub fn city(&self) -> &str {
        &self.city
    }

    /// The two-letter abbreviation, in upper case.
    pub fn state(&self) -> &str {
        &self.state
    }
}

fn invalid(field: &'static str) -> DomainError {
    DomainError::InvalidAddressField(field)
}

/// A Brazilian postal code (CEP): eight digits, written with or without the hyphen (`01310-100`)
/// and kept without it. Only the shape is checked; whether the CEP exists is a lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostalCode(String);

impl PostalCode {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let code: String = raw
            .chars()
            .filter(|c| *c != '-' && !c.is_whitespace())
            .collect();
        if code.len() == POSTAL_CODE_LEN && code.bytes().all(|byte| byte.is_ascii_digit()) {
            Ok(Self(code))
        } else {
            Err(DomainError::InvalidPostalCode)
        }
    }

    /// Eight digits, without the hyphen.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether the text is one of the 27 state abbreviations, in upper case.
pub fn is_state(abbreviation: &str) -> bool {
    STATES.contains(&abbreviation)
}

fn clean_state(raw: &str) -> Result<String, DomainError> {
    let state = raw.trim().to_ascii_uppercase();
    if STATES.contains(&state.as_str()) {
        Ok(state)
    } else {
        Err(DomainError::InvalidState)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> AddressDraft {
        AddressDraft {
            postal_code: "01310-100".into(),
            street: "  Avenida Paulista ".into(),
            number: Some(" 1000 ".into()),
            complement: Some("conj. 12".into()),
            district: Some("Bela Vista".into()),
            city: "São Paulo".into(),
            state: " sp ".into(),
        }
    }

    #[test]
    fn keeps_one_form_of_the_postal_code_and_the_state() {
        let address = Address::new(draft()).unwrap();
        assert_eq!(address.postal_code(), "01310100");
        assert_eq!(address.state(), "SP");
        assert_eq!(address.street(), "Avenida Paulista");
        assert_eq!(address.number(), Some("1000"));
    }

    #[test]
    fn the_number_the_complement_and_the_district_are_optional_and_blank_is_not_set() {
        let address = Address::new(AddressDraft {
            number: Some("  ".into()),
            complement: None,
            district: Some("".into()),
            ..draft()
        })
        .unwrap();
        assert_eq!(address.number(), None);
        assert_eq!(address.complement(), None);
        assert_eq!(address.district(), None);
    }

    #[test]
    fn a_postal_code_has_eight_digits() {
        for raw in ["", "0131010", "013101000", "01310-10a", "0131 0100 0"] {
            let result = Address::new(AddressDraft {
                postal_code: raw.into(),
                ..draft()
            });
            assert_eq!(result, Err(DomainError::InvalidPostalCode), "{raw}");
        }
    }

    #[test]
    fn the_state_is_one_of_the_twenty_seven() {
        for raw in ["", "S", "SPP", "XX", "sã"] {
            let result = Address::new(AddressDraft {
                state: raw.into(),
                ..draft()
            });
            assert_eq!(result, Err(DomainError::InvalidState), "{raw}");
        }
        assert!(STATES.iter().all(|state| {
            Address::new(AddressDraft {
                state: (*state).into(),
                ..draft()
            })
            .is_ok()
        }));
    }

    #[test]
    fn the_street_and_the_city_are_required_and_bounded_and_the_error_names_the_field() {
        let blank_street = Address::new(AddressDraft {
            street: "  ".into(),
            ..draft()
        });
        assert_eq!(
            blank_street,
            Err(DomainError::InvalidAddressField("street"))
        );
        let long_city = Address::new(AddressDraft {
            city: "a".repeat(81),
            ..draft()
        });
        assert_eq!(long_city, Err(DomainError::InvalidAddressField("city")));
        let nul = Address::new(AddressDraft {
            complement: Some("sala\0 3".into()),
            ..draft()
        });
        assert_eq!(nul, Err(DomainError::InvalidAddressField("complement")));
    }
}
