/// Situation Receita Federal reports for a company that can issue and receive documents.
const ACTIVE_SITUATION: &str = "ATIVA";

/// What is known about a party's document beyond its check digits. Only a CNPJ can be looked up in
/// the federal registry; a CPF is validated by its digits alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentCheck {
    /// Nothing to claim: no document, a CPF, or the operator switched the CNPJ lookup off.
    NotApplicable,
    /// The CNPJ exists in the registry. `situation` is the registry's own wording ("ATIVA", "BAIXADA", ...).
    Verified {
        legal_name: String,
        situation: String,
    },
    /// The registry answered that it has no such company. Not proof the company does not exist:
    /// the open data lags, so a recently opened company can be missing. It may also be a typo.
    NotFound,
    /// The registry could not be reached, so only the check digits were validated.
    Unverified,
}

impl DocumentCheck {
    /// Whether the user should be told about this party's document: the registry does not know
    /// the company, could not be reached, or the company is not active (closed, suspended, ...).
    /// It never blocks registering.
    pub fn needs_attention(&self) -> bool {
        match self {
            Self::NotApplicable => false,
            Self::Verified { situation, .. } => !situation.eq_ignore_ascii_case(ACTIVE_SITUATION),
            Self::NotFound | Self::Unverified => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verified(situation: &str) -> DocumentCheck {
        DocumentCheck::Verified {
            legal_name: "PADARIA CENTRAL LTDA".into(),
            situation: situation.into(),
        }
    }

    #[test]
    fn only_unreachable_or_inactive_companies_need_attention() {
        assert!(!DocumentCheck::NotApplicable.needs_attention());
        assert!(!verified("ATIVA").needs_attention());
        assert!(!verified("Ativa").needs_attention());
        assert!(verified("BAIXADA").needs_attention());
        assert!(verified("SUSPENSA").needs_attention());
        assert!(DocumentCheck::Unverified.needs_attention());
        assert!(DocumentCheck::NotFound.needs_attention());
    }
}
