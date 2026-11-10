use conta_verde_application::RepositoryError;
use conta_verde_domain::{
    Actor, ApiKeyId, AuditAction, AuditEntity, AuditEntry, AuditRecord, CompanyId, FieldChange,
    UserId,
};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

use crate::postgres::storage_error;

/// The shape of one change inside the `changes` column.
#[derive(Serialize, Deserialize)]
struct StoredChange {
    field: String,
    from: Option<String>,
    to: Option<String>,
}

/// Inserts the entry on the connection of the business write's transaction, so both commit or
/// neither does.
pub(crate) async fn insert(
    connection: &mut PgConnection,
    audit: &AuditEntry,
) -> Result<(), RepositoryError> {
    let changes: Vec<StoredChange> = audit
        .changes
        .iter()
        .map(|change| StoredChange {
            field: change.field.clone(),
            from: change.from.clone(),
            to: change.to.clone(),
        })
        .collect();
    let changes = serde_json::to_string(&changes).map_err(|error| {
        RepositoryError::Storage(format!("cannot encode audit changes: {error}"))
    })?;

    // Bound as text and cast, which avoids enabling sqlx's `json` feature for one column.
    sqlx::query(
        "INSERT INTO audit_log (company_id, actor_kind, actor_id, actor_name, entity, entity_id, action, changes) \
         VALUES ($1, $8, $2, $3, $4, $5, $6, CAST($7 AS jsonb))",
    )
    .bind(audit.company_id.as_uuid())
    .bind(audit.actor.id())
    .bind(audit.actor.name())
    .bind(audit.entity.as_str())
    .bind(audit.entity_id)
    .bind(audit.action.as_str())
    .bind(changes)
    .bind(audit.actor.kind())
    .execute(connection)
    .await
    .map_err(storage_error)?;
    Ok(())
}

/// The trail of one record, oldest first. Time is read as epoch milliseconds so no date-time
/// library is needed; the API and the SPA format it.
pub(crate) async fn history(
    pool: &PgPool,
    company_id: CompanyId,
    entity: AuditEntity,
    entity_id: Uuid,
) -> Result<Vec<AuditRecord>, RepositoryError> {
    let rows = sqlx::query(
        "SELECT sequence, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS at_millis, \
                actor_kind, actor_id, actor_name, action, changes::text AS changes \
         FROM audit_log \
         WHERE company_id = $1 AND entity = $2 AND entity_id = $3 \
         ORDER BY sequence",
    )
    .bind(company_id.as_uuid())
    .bind(entity.as_str())
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .map_err(storage_error)?;

    rows.iter()
        .map(|row| record_from_row(row, company_id, entity, entity_id))
        .collect()
}

fn record_from_row(
    row: &PgRow,
    company_id: CompanyId,
    entity: AuditEntity,
    entity_id: Uuid,
) -> Result<AuditRecord, RepositoryError> {
    let action: String = row.try_get("action").map_err(storage_error)?;
    let action = AuditAction::parse(&action)
        .ok_or_else(|| RepositoryError::Storage(format!("unknown audit action `{action}`")))?;
    let raw_changes: String = row.try_get("changes").map_err(storage_error)?;
    let stored: Vec<StoredChange> = serde_json::from_str(&raw_changes)
        .map_err(|error| RepositoryError::Storage(format!("unreadable audit changes: {error}")))?;

    Ok(AuditRecord {
        sequence: row.try_get("sequence").map_err(storage_error)?,
        at_millis: row.try_get("at_millis").map_err(storage_error)?,
        entry: AuditEntry {
            company_id,
            actor: actor_from_row(row)?,
            entity,
            entity_id,
            action,
            changes: stored
                .into_iter()
                .map(|change| FieldChange {
                    field: change.field,
                    from: change.from,
                    to: change.to,
                })
                .collect(),
        },
    })
}

fn actor_from_row(row: &PgRow) -> Result<Actor, RepositoryError> {
    let id: Uuid = row.try_get("actor_id").map_err(storage_error)?;
    let name: String = row.try_get("actor_name").map_err(storage_error)?;
    let kind: String = row.try_get("actor_kind").map_err(storage_error)?;
    match kind.as_str() {
        "user" => Ok(Actor::User {
            id: UserId::from_uuid(id),
            name,
        }),
        "api_key" => Ok(Actor::ApiKey {
            id: ApiKeyId::from_uuid(id),
            name,
        }),
        other => Err(RepositoryError::Storage(format!(
            "unknown audit actor kind `{other}`"
        ))),
    }
}
