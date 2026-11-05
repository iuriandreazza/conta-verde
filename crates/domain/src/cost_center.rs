use crate::name::clean_name;
use crate::{CompanyId, CostCenterId, DomainError};

/// Unvalidated input for registering or changing a cost center.
#[derive(Debug, Clone)]
pub struct CostCenterDraft {
    pub name: String,
    pub active: bool,
}

/// Where a cost is incurred, such as a store, a project or a department, owned by one company.
/// Entries will be tagged with one, so that results can be read per cost center.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostCenter {
    id: CostCenterId,
    company_id: CompanyId,
    name: String,
    active: bool,
}

impl CostCenter {
    pub fn new(
        id: CostCenterId,
        company_id: CompanyId,
        draft: CostCenterDraft,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            company_id,
            name: clean_name(&draft.name)?,
            active: draft.active,
        })
    }

    /// The same cost center with every editable field replaced by the draft's.
    pub fn replaced_with(&self, draft: CostCenterDraft) -> Result<Self, DomainError> {
        Self::new(self.id, self.company_id, draft)
    }

    pub fn id(&self) -> CostCenterId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
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

    fn build(name: &str, active: bool) -> Result<CostCenter, DomainError> {
        CostCenter::new(
            CostCenterId::generate(),
            CompanyId::generate(),
            CostCenterDraft {
                name: name.into(),
                active,
            },
        )
    }

    #[test]
    fn keeps_a_trimmed_name_and_refuses_a_blank_or_unstorable_one() {
        assert_eq!(build("  Loja Centro ", true).unwrap().name(), "Loja Centro");
        assert_eq!(build("  ", true).unwrap_err(), DomainError::EmptyName);
        assert_eq!(build("Loja\0", true).unwrap_err(), DomainError::InvalidName);
        assert_eq!(
            build(&"x".repeat(201), true).unwrap_err(),
            DomainError::InvalidName
        );
    }

    #[test]
    fn replacing_keeps_the_identity() {
        let center = build("Loja", true).unwrap();
        let replaced = center
            .replaced_with(CostCenterDraft {
                name: "Loja Centro".into(),
                active: false,
            })
            .unwrap();
        assert_eq!(replaced.id(), center.id());
        assert_eq!(replaced.company_id(), center.company_id());
        assert_eq!(replaced.name(), "Loja Centro");
        assert!(!replaced.is_active());
    }
}
