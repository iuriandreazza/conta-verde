use crate::name::clean_name;
use crate::{
    Address, AddressDraft, CompanyId, Document, DocumentCheck, DomainError, Email, PartyId,
};

/// Unvalidated input for registering or changing a party.
#[derive(Debug, Clone, Default)]
pub struct PartyDraft {
    pub name: String,
    pub document: Option<String>,
    pub email: Option<String>,
    pub is_customer: bool,
    pub is_supplier: bool,
    pub is_carrier: bool,
    pub is_seller: bool,
    pub address: Option<AddressDraft>,
}

/// A customer, supplier, carrier and/or seller owned by a single company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    id: PartyId,
    company_id: CompanyId,
    name: String,
    document: Option<Document>,
    email: Option<Email>,
    is_customer: bool,
    is_supplier: bool,
    is_carrier: bool,
    is_seller: bool,
    address: Option<Address>,
    document_check: DocumentCheck,
}

impl Party {
    pub fn new(id: PartyId, company_id: CompanyId, draft: PartyDraft) -> Result<Self, DomainError> {
        let name = clean_name(&draft.name)?;
        if !(draft.is_customer || draft.is_supplier || draft.is_carrier || draft.is_seller) {
            return Err(DomainError::MissingRole);
        }
        let document = draft.document.as_deref().map(Document::parse).transpose()?;
        let email = draft.email.as_deref().map(Email::parse).transpose()?;
        let address = draft.address.map(Address::new).transpose()?;

        Ok(Self {
            id,
            company_id,
            name,
            document,
            email,
            is_customer: draft.is_customer,
            is_supplier: draft.is_supplier,
            is_carrier: draft.is_carrier,
            is_seller: draft.is_seller,
            address,
            document_check: DocumentCheck::NotApplicable,
        })
    }

    /// The same party with every editable field replaced by the draft's, validated as for a new
    /// one. What the registry said stays only while the document is the same: a new document has
    /// not been checked yet, and the use case asks the registry about it.
    pub fn replaced_with(&self, draft: PartyDraft) -> Result<Self, DomainError> {
        let mut party = Self::new(self.id, self.company_id, draft)?;
        if party.document == self.document {
            party.document_check = self.document_check.clone();
        }
        Ok(party)
    }

    /// Records the outcome of looking the CNPJ up in the federal registry. Only a CNPJ is ever
    /// looked up, so any other document keeps `NotApplicable`.
    pub fn with_document_check(mut self, check: DocumentCheck) -> Self {
        debug_assert!(
            check == DocumentCheck::NotApplicable
                || self.document.as_ref().and_then(Document::cnpj).is_some(),
            "only a CNPJ can have a registry check"
        );
        self.document_check = check;
        self
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

    pub fn email(&self) -> Option<&Email> {
        self.email.as_ref()
    }

    pub fn is_customer(&self) -> bool {
        self.is_customer
    }

    pub fn is_supplier(&self) -> bool {
        self.is_supplier
    }

    pub fn is_carrier(&self) -> bool {
        self.is_carrier
    }

    pub fn is_seller(&self) -> bool {
        self.is_seller
    }

    pub fn address(&self) -> Option<&Address> {
        self.address.as_ref()
    }

    pub fn document_check(&self) -> &DocumentCheck {
        &self.document_check
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
            ..PartyDraft::default()
        }
    }

    fn build(draft: PartyDraft) -> Result<Party, DomainError> {
        Party::new(PartyId::generate(), CompanyId::generate(), draft)
    }

    #[test]
    fn trims_name_and_normalizes_document() {
        let party = build(draft()).unwrap();
        assert_eq!(party.name(), "Padaria Central");
        assert_eq!(party.document().unwrap().as_str(), "11222333000181");
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
            ..draft()
        });
        assert_eq!(result, Err(DomainError::MissingRole));
    }

    #[test]
    fn a_carrier_or_a_seller_alone_is_enough() {
        for (is_carrier, is_seller) in [(true, false), (false, true)] {
            let party = build(PartyDraft {
                is_customer: false,
                is_carrier,
                is_seller,
                ..draft()
            })
            .unwrap();
            assert_eq!(
                (party.is_carrier(), party.is_seller()),
                (is_carrier, is_seller)
            );
        }
    }

    #[test]
    fn an_address_is_optional_and_checked_when_given() {
        assert!(build(draft()).unwrap().address().is_none());
        let with_address = |postal_code: &str| {
            build(PartyDraft {
                address: Some(AddressDraft {
                    postal_code: postal_code.into(),
                    street: "Rua A".into(),
                    city: "Recife".into(),
                    state: "PE".into(),
                    ..AddressDraft::default()
                }),
                ..draft()
            })
        };
        assert_eq!(
            with_address("50000-000")
                .unwrap()
                .address()
                .unwrap()
                .postal_code(),
            "50000000"
        );
        assert_eq!(
            with_address("5000").unwrap_err(),
            DomainError::InvalidPostalCode
        );
    }

    #[test]
    fn replacing_keeps_the_identity_and_the_registry_answer_only_for_the_same_document() {
        let verified = DocumentCheck::Verified {
            legal_name: "PADARIA CENTRAL LTDA".into(),
            situation: "ATIVA".into(),
        };
        let party = build(draft())
            .unwrap()
            .with_document_check(verified.clone());

        let renamed = party
            .replaced_with(PartyDraft {
                name: "Padaria do Centro".into(),
                ..draft()
            })
            .unwrap();
        assert_eq!(
            (renamed.id(), renamed.company_id()),
            (party.id(), party.company_id())
        );
        assert_eq!(renamed.document_check(), &verified);

        let other_document = party
            .replaced_with(PartyDraft {
                document: Some("11.444.777/0001-61".into()),
                ..draft()
            })
            .unwrap();
        assert_eq!(
            other_document.document_check(),
            &DocumentCheck::NotApplicable
        );
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
