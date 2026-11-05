use uuid::Uuid;

use crate::{CompanyId, DocumentCheck, Party, UserId};

/// Who made a change. The name is a snapshot: history must keep reading the same after the user is
/// renamed or removed. API keys will join as another variant ([ADR 0007]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    User { id: UserId, name: String },
}

/// The kind of record that changed. One variant per audited table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEntity {
    Party,
}

impl AuditEntity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Party => "party",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditAction {
    Created,
    Updated,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "created" => Some(Self::Created),
            "updated" => Some(Self::Updated),
            _ => None,
        }
    }
}

/// One field that changed. Values are text so the trail reads the same whatever the field's type;
/// `None` means "not set".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    pub field: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// What the store is asked to record together with a write. The store adds the sequence and time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub company_id: CompanyId,
    pub actor: Actor,
    pub entity: AuditEntity,
    pub entity_id: Uuid,
    pub action: AuditAction,
    pub changes: Vec<FieldChange>,
}

impl AuditEntry {
    /// Whether this entry is about that party, in that party's company. Stores check it before
    /// writing, so a caller's mistake cannot put a row into another company's trail.
    pub fn describes(&self, party: &Party) -> bool {
        self.entity == AuditEntity::Party
            && self.company_id == party.company_id()
            && self.entity_id == party.id().as_uuid()
    }
}

/// An entry as read back. `sequence` orders records of the whole trail, also within the same
/// millisecond; `at_millis` is milliseconds since the Unix epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub sequence: i64,
    pub at_millis: i64,
    pub entry: AuditEntry,
}

fn set(field: &str, value: impl Into<String>) -> FieldChange {
    FieldChange {
        field: field.to_owned(),
        from: None,
        to: Some(value.into()),
    }
}

/// Every field a new party has set.
pub fn creation_changes(party: &Party) -> Vec<FieldChange> {
    let mut changes = vec![set("name", party.name())];
    if let Some(document) = party.document() {
        changes.push(set("document", document.as_str()));
    }
    if let Some(email) = party.email() {
        changes.push(set("email", email.as_str()));
    }
    changes.push(set("is_customer", party.is_customer().to_string()));
    changes.push(set("is_supplier", party.is_supplier().to_string()));
    changes.extend(document_check_changes(
        &DocumentCheck::NotApplicable,
        party.document_check(),
    ));
    changes
}

/// The fields of the registry check that differ between two answers; empty when nothing changed.
pub fn document_check_changes(before: &DocumentCheck, after: &DocumentCheck) -> Vec<FieldChange> {
    let fields = |check: &DocumentCheck| -> [(&'static str, Option<String>); 3] {
        let (kind, legal_name, situation) = match check {
            DocumentCheck::NotApplicable => (None, None, None),
            DocumentCheck::NotFound => (Some("not_found"), None, None),
            DocumentCheck::Unverified => (Some("unverified"), None, None),
            DocumentCheck::Verified {
                legal_name,
                situation,
            } => (
                Some("verified"),
                Some(legal_name.clone()),
                Some(situation.clone()),
            ),
        };
        [
            ("document_check", kind.map(str::to_owned)),
            ("registry_legal_name", legal_name),
            ("registry_situation", situation),
        ]
    };
    fields(before)
        .into_iter()
        .zip(fields(after))
        .filter(|((_, from), (_, to))| from != to)
        .map(|((field, from), (_, to))| FieldChange {
            field: field.to_owned(),
            from,
            to,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompanyId, PartyDraft, PartyId};

    fn party(document: Option<&str>) -> Party {
        Party::new(
            PartyId::generate(),
            CompanyId::generate(),
            PartyDraft {
                name: "Padaria Central".into(),
                document: document.map(Into::into),
                email: Some("contato@padaria.example".into()),
                is_customer: true,
                is_supplier: false,
            },
        )
        .unwrap()
    }

    fn fields(changes: &[FieldChange]) -> Vec<&str> {
        changes.iter().map(|change| change.field.as_str()).collect()
    }

    #[test]
    fn a_new_party_records_only_the_fields_it_has() {
        let bare = party(None);
        let changes = creation_changes(&bare);
        assert_eq!(
            fields(&changes),
            ["name", "email", "is_customer", "is_supplier"]
        );
        assert!(changes.iter().all(|change| change.from.is_none()));

        let with_document = party(Some("529.982.247-25"));
        assert!(fields(&creation_changes(&with_document)).contains(&"document"));
    }

    #[test]
    fn a_new_party_records_what_the_registry_said() {
        let checked =
            party(Some("11.222.333/0001-81")).with_document_check(DocumentCheck::Verified {
                legal_name: "PADARIA CENTRAL LTDA".into(),
                situation: "ATIVA".into(),
            });
        let changes = creation_changes(&checked);
        assert!(changes.contains(&set("document_check", "verified")));
        assert!(changes.contains(&set("registry_situation", "ATIVA")));
    }

    #[test]
    fn a_recheck_lists_only_what_changed() {
        let verified = DocumentCheck::Verified {
            legal_name: "PADARIA CENTRAL LTDA".into(),
            situation: "ATIVA".into(),
        };
        let closed = DocumentCheck::Verified {
            legal_name: "PADARIA CENTRAL LTDA".into(),
            situation: "BAIXADA".into(),
        };

        let changes = document_check_changes(&verified, &closed);
        assert_eq!(
            changes,
            [FieldChange {
                field: "registry_situation".into(),
                from: Some("ATIVA".into()),
                to: Some("BAIXADA".into()),
            }]
        );

        let from_unverified = document_check_changes(&DocumentCheck::Unverified, &verified);
        assert_eq!(
            fields(&from_unverified),
            [
                "document_check",
                "registry_legal_name",
                "registry_situation"
            ]
        );
        assert_eq!(from_unverified[0].from.as_deref(), Some("unverified"));
        assert_eq!(from_unverified[0].to.as_deref(), Some("verified"));
    }

    #[test]
    fn the_same_answer_changes_nothing() {
        assert!(
            document_check_changes(&DocumentCheck::Unverified, &DocumentCheck::Unverified)
                .is_empty()
        );
        assert!(
            document_check_changes(&DocumentCheck::NotFound, &DocumentCheck::NotFound).is_empty()
        );
    }
}
