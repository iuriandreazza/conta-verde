use conta_verde_application::{BankConnectionRepository, RepositoryError, ensure_audit_describes};
use conta_verde_domain::{
    Actor, AuditEntity, AuditEntry, AuditRecord, BankAccountId, BankConnection, BankConnectionId,
    CalendarDate, CompanyId, ConnectionStatus, UserId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use crate::postgres::storage_error;
use crate::postgres_audit;

const LIVE_UNIQUE_INDEXES: [&str; 2] = [
    "bank_connections_one_live_key",
    "bank_connections_one_live_account",
];

const SELECT_CONNECTIONS: &str = "SELECT id, company_id, bank_account_id, provider, item_id, account_id, status, \
     to_char(last_synced_on, 'YYYY-MM-DD') AS last_synced_on, last_error, created_by_id, created_by_name, created_at \
     FROM bank_connections";

pub struct PostgresBankConnectionRepository {
    pool: PgPool,
}

impl PostgresBankConnectionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn connection_from_row(row: &PgRow) -> Result<BankConnection, RepositoryError> {
    let status: String = row.try_get("status").map_err(storage_error)?;
    let status = ConnectionStatus::parse(&status).ok_or_else(|| {
        RepositoryError::Storage(format!("stored bank connection has status {status:?}"))
    })?;
    let day: Option<String> = row.try_get("last_synced_on").map_err(storage_error)?;
    Ok(BankConnection::restored(
        BankConnectionId::from_uuid(row.try_get("id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        BankAccountId::from_uuid(row.try_get("bank_account_id").map_err(storage_error)?),
        row.try_get("provider").map_err(storage_error)?,
        row.try_get("item_id").map_err(storage_error)?,
        row.try_get("account_id").map_err(storage_error)?,
        status,
        day.as_deref().and_then(CalendarDate::parse),
        row.try_get("last_error").map_err(storage_error)?,
        Actor::User {
            id: UserId::from_uuid(row.try_get("created_by_id").map_err(storage_error)?),
            name: row.try_get("created_by_name").map_err(storage_error)?,
        },
        row.try_get("created_at").map_err(storage_error)?,
    ))
}

impl BankConnectionRepository for PostgresBankConnectionRepository {
    async fn save(
        &self,
        connection: &BankConnection,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, connection)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO bank_connections (id, company_id, bank_account_id, provider, item_id, account_id, status, \
                 created_by_id, created_by_name, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(connection.id().as_uuid())
        .bind(connection.company_id().as_uuid())
        .bind(connection.bank_account_id().as_uuid())
        .bind(connection.provider())
        .bind(connection.item_id())
        .bind(connection.account_id())
        .bind(connection.status().as_str())
        .bind(connection.connected_by().id())
        .bind(connection.connected_by().name())
        .bind(connection.created_at())
        .execute(&mut *tx)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db)
                if db
                    .constraint()
                    .is_some_and(|name| LIVE_UNIQUE_INDEXES.contains(&name)) =>
            {
                RepositoryError::DuplicateBankConnection
            }
            _ => storage_error(error),
        })?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn update_status(
        &self,
        connection: &BankConnection,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, connection)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // Only the status and its reason: the day a pull reached is not overwritten with an older copy. An ended connection stays
        // ended: a pull that was half way when it was ended must not bring it back.
        let result = sqlx::query(
            "UPDATE bank_connections SET status = $3, last_error = $4 \
             WHERE id = $1 AND company_id = $2 AND (status <> 'disconnected' OR $3 = 'disconnected')",
        )
        .bind(connection.id().as_uuid())
        .bind(connection.company_id().as_uuid())
        .bind(connection.status().as_str())
        .bind(connection.last_error())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn record_sync(&self, connection: &BankConnection) -> Result<(), RepositoryError> {
        // The day only moves forward, and a failure leaves the status alone.
        let result = sqlx::query(
            "UPDATE bank_connections SET \
                 last_synced_on = GREATEST(last_synced_on, CAST($3 AS DATE)), last_error = $4 \
             WHERE id = $1 AND company_id = $2",
        )
        .bind(connection.id().as_uuid())
        .bind(connection.company_id().as_uuid())
        .bind(connection.last_synced_on().map(|day| day.to_string()))
        .bind(connection.last_error())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: BankConnectionId,
    ) -> Result<Option<BankConnection>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_CONNECTIONS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(connection_from_row).transpose()
    }

    async fn find_by_id(
        &self,
        id: BankConnectionId,
    ) -> Result<Option<BankConnection>, RepositoryError> {
        let row = sqlx::query(&format!("{SELECT_CONNECTIONS} WHERE id = $1"))
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;
        row.as_ref().map(connection_from_row).transpose()
    }

    async fn list(&self, company_id: CompanyId) -> Result<Vec<BankConnection>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_CONNECTIONS} WHERE company_id = $1 ORDER BY created_at DESC, id DESC"
        ))
        .bind(company_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(connection_from_row).collect()
    }

    async fn due(
        &self,
        day: CalendarDate,
        limit: usize,
    ) -> Result<Vec<BankConnection>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_CONNECTIONS} WHERE status = 'connected' AND (last_synced_on IS NULL OR last_synced_on < CAST($1 AS DATE)) \
             ORDER BY last_synced_on NULLS FIRST, id LIMIT $2"
        ))
        .bind(day.to_string())
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(connection_from_row).collect()
    }

    async fn item_is_live(&self, provider: &str, item_id: &str) -> Result<bool, RepositoryError> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM bank_connections \
             WHERE provider = $1 AND item_id = $2 AND status <> 'disconnected')",
        )
        .bind(provider)
        .bind(item_id)
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: BankConnectionId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::BankConnection,
            id.as_uuid(),
        )
        .await
    }
}
