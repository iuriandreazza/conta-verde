use conta_verde_domain::AuditRecord;
use serde::Serialize;
use uuid::Uuid;

/// What was recorded about a record, oldest first. The array order is the order of the trail; the
/// store's own counter is not exposed, because it is shared by every company and would show how
/// busy the installation is.
#[derive(Serialize)]
pub(crate) struct HistoryEntryResponse {
    /// Milliseconds since the Unix epoch; the client formats it in the user's time zone.
    at: i64,
    actor: ActorResponse,
    action: &'static str,
    changes: Vec<ChangeResponse>,
}

#[derive(Serialize)]
struct ActorResponse {
    kind: &'static str,
    id: Uuid,
    /// The name the actor had when they made the change.
    name: String,
}

#[derive(Serialize)]
struct ChangeResponse {
    field: String,
    from: Option<String>,
    to: Option<String>,
}

impl From<&AuditRecord> for HistoryEntryResponse {
    fn from(record: &AuditRecord) -> Self {
        let actor = &record.entry.actor;
        Self {
            at: record.at_millis,
            actor: ActorResponse {
                kind: actor.kind(),
                id: actor.id(),
                name: actor.name().to_owned(),
            },
            action: record.entry.action.as_str(),
            changes: record
                .entry
                .changes
                .iter()
                .map(|change| ChangeResponse {
                    field: change.field.clone(),
                    from: change.from.clone(),
                    to: change.to.clone(),
                })
                .collect(),
        }
    }
}
