use crate::{CompanyId, Document, DomainError, PartyId};

/// Unvalidated input for registering a customer and/or supplier.
#[derive(Debug, Clone)]
pub struct PartyDraft {
    pub name: String,
    pub document: Option<String>,
    pub email: Option<String>,
    pub is_customer: bool,
    pub is_supplier: bool,
}

/// A customer and/or supplier owned by a single company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    id: PartyId,
    company_id: CompanyId,
    name: String,
    document: Option<Document>,
    email: Option<String>,
    is_customer: bool,
    is_supplier: bool,
}

impl Party {
    pub fn new(id: PartyId, company_id: CompanyId, draft: PartyDraft) -> Result<Self, DomainError> {
        let name = draft.name.trim().to_owned();
        if name.is_empty() {
            return Err(DomainError::EmptyName);
        }
        if !draft.is_customer && !draft.is_supplier {
            return Err(DomainError::MissingRole);
        }
        let document = draft.document.as_deref().map(Document::parse).transpose()?;
        let email = draft.email.as_deref().map(parse_email).transpose()?;

        Ok(Self {
            id,
            company_id,
            name,
            document,
            email,
            is_customer: draft.is_customer,
            is_supplier: draft.is_supplier,
        })
    }

    pub fn id(&self) -> PartyId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn is_customer(&self) -> bool {
        self.is_customer
    }

    pub fn is_supplier(&self) -> bool {
        self.is_supplier
    }
}

fn parse_email(raw: &str) -> Result<String, DomainError> {
    let email = raw.trim();
    match email.split_once('@') {
        Some((local, domain)) if !local.is_empty() && domain.contains('.') => Ok(email.to_owned()),
        _ => Err(DomainError::InvalidEmail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> PartyDraft {
        PartyDraft {
            name: "  Padaria Central  ".into(),
            document: Some("11.222.333/0001-81".into()),
            email: Some("contato@padaria.example".into()),
            is_customer: true,
            is_supplier: false,
        }
    }

    fn build(draft: PartyDraft) -> Result<Party, DomainError> {
        Party::new(PartyId::generate(), CompanyId::generate(), draft)
    }

    #[test]
    fn trims_name_and_normalizes_document() {
        let party = build(draft()).unwrap();
        assert_eq!(party.name(), "Padaria Central");
        assert_eq!(party.document().unwrap().digits(), "11222333000181");
    }

    #[test]
    fn rejects_blank_name() {
        let result = build(PartyDraft {
            name: "   ".into(),
            ..draft()
        });
        assert_eq!(result, Err(DomainError::EmptyName));
    }

    #[test]
    fn requires_a_role() {
        let result = build(PartyDraft {
            is_customer: false,
            is_supplier: false,
            ..draft()
        });
        assert_eq!(result, Err(DomainError::MissingRole));
    }

    #[test]
    fn rejects_malformed_email() {
        let result = build(PartyDraft {
            email: Some("not-an-email".into()),
            ..draft()
        });
        assert_eq!(result, Err(DomainError::InvalidEmail));
    }

    #[test]
    fn document_and_email_are_optional() {
        let result = build(PartyDraft {
            document: None,
            email: None,
            ..draft()
        });
        assert!(result.is_ok());
    }
}
