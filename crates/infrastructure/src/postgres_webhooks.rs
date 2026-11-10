use conta_verde_application::{
    DeliveryCursor, RepositoryError, WebhookRepository, WebhookSecrets, ensure_audit_describes,
};
use conta_verde_domain::{
    Actor, AuditEntity, AuditEntry, AuditRecord, CompanyId, DeliveryStatus, SealedSecret, UserId,
    WebhookDelivery, WebhookDeliveryId, WebhookEventKind, WebhookSubscription,
    WebhookSubscriptionId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::postgres::storage_error;
use crate::postgres_audit;

const SELECT_SUBSCRIPTIONS: &str = "SELECT id, company_id, url, events, active, created_by_id, \
     created_by_name, created_at FROM webhook_subscriptions";

const SELECT_DELIVERIES: &str = "SELECT id, company_id, subscription_id, event_id, kind, payload, \
     status, attempts, last_status, last_error, created_at, updated_at, delivered_at \
     FROM webhook_deliveries";

pub struct PostgresWebhookRepository {
    pool: PgPool,
}

impl PostgresWebhookRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn corrupt(what: &str, detail: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::Storage(format!("stored webhook {what} is invalid: {detail}"))
}

fn subscription_from_row(row: &PgRow) -> Result<WebhookSubscription, RepositoryError> {
    let events: Vec<String> = row.try_get("events").map_err(storage_error)?;
    let events = events
        .iter()
        .map(|text| WebhookEventKind::parse(text).ok_or_else(|| corrupt("event", text)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WebhookSubscription::restored(
        WebhookSubscriptionId::from_uuid(row.try_get("id").map_err(storage_error)?),
        CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        row.try_get("url").map_err(storage_error)?,
        events,
        row.try_get("active").map_err(storage_error)?,
        Actor::User {
            id: UserId::from_uuid(row.try_get("created_by_id").map_err(storage_error)?),
            name: row.try_get("created_by_name").map_err(storage_error)?,
        },
        row.try_get("created_at").map_err(storage_error)?,
    ))
}

fn delivery_from_row(row: &PgRow) -> Result<WebhookDelivery, RepositoryError> {
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let status: String = row.try_get("status").map_err(storage_error)?;
    let attempts: i32 = row.try_get("attempts").map_err(storage_error)?;
    let last_status: Option<i32> = row.try_get("last_status").map_err(storage_error)?;
    Ok(WebhookDelivery {
        id: WebhookDeliveryId::from_uuid(row.try_get("id").map_err(storage_error)?),
        company_id: CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?),
        subscription_id: WebhookSubscriptionId::from_uuid(
            row.try_get("subscription_id").map_err(storage_error)?,
        ),
        event_id: row.try_get("event_id").map_err(storage_error)?,
        kind: WebhookEventKind::parse(&kind).ok_or_else(|| corrupt("kind", &kind))?,
        payload: row.try_get("payload").map_err(storage_error)?,
        status: DeliveryStatus::parse(&status).ok_or_else(|| corrupt("status", &status))?,
        attempts: u32::try_from(attempts).unwrap_or(0),
        last_status: last_status.and_then(|code| u16::try_from(code).ok()),
        last_error: row.try_get("last_error").map_err(storage_error)?,
        created_at: row.try_get("created_at").map_err(storage_error)?,
        updated_at: row.try_get("updated_at").map_err(storage_error)?,
        delivered_at: row.try_get("delivered_at").map_err(storage_error)?,
    })
}

fn events_of(subscription: &WebhookSubscription) -> Vec<String> {
    subscription
        .events()
        .iter()
        .map(|kind| kind.as_str().to_owned())
        .collect()
}

/// Makes one delivery unless the subscription already has one for the event: the unique key is the claim.
async fn insert_delivery(
    pool: &PgPool,
    subscription: &WebhookSubscription,
    event_id: Uuid,
    kind: WebhookEventKind,
    payload: &str,
    now: i64,
) -> Result<Option<WebhookDelivery>, RepositoryError> {
    let row = sqlx::query(
        "INSERT INTO webhook_deliveries (id, company_id, subscription_id, event_id, kind, payload, status, attempts, \
             created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 'pending', 0, $7, $7) \
         ON CONFLICT (subscription_id, event_id) DO NOTHING \
         RETURNING id, company_id, subscription_id, event_id, kind, payload, status, attempts, last_status, last_error, \
             created_at, updated_at, delivered_at",
    )
    .bind(WebhookDeliveryId::generate().as_uuid())
    .bind(subscription.company_id().as_uuid())
    .bind(subscription.id().as_uuid())
    .bind(event_id)
    .bind(kind.as_str())
    .bind(payload)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(storage_error)?;
    row.as_ref().map(delivery_from_row).transpose()
}

impl WebhookRepository for PostgresWebhookRepository {
    async fn save(
        &self,
        subscription: &WebhookSubscription,
        secret: &SealedSecret,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, subscription)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO webhook_subscriptions (id, company_id, url, events, active, secret_sealed, created_by_id, \
                 created_by_name, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(subscription.id().as_uuid())
        .bind(subscription.company_id().as_uuid())
        .bind(subscription.url())
        .bind(events_of(subscription))
        .bind(subscription.is_active())
        .bind(secret.as_bytes())
        .bind(subscription.created_by().id())
        .bind(subscription.created_by().name())
        .bind(subscription.created_at())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn update(
        &self,
        subscription: &WebhookSubscription,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, subscription)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE webhook_subscriptions SET url = $3, events = $4, active = $5 WHERE id = $1 AND company_id = $2",
        )
        .bind(subscription.id().as_uuid())
        .bind(subscription.company_id().as_uuid())
        .bind(subscription.url())
        .bind(events_of(subscription))
        .bind(subscription.is_active())
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn replace_secret(
        &self,
        subscription: &WebhookSubscription,
        secret: &SealedSecret,
        previous_until: i64,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, subscription)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        // The current secret becomes the previous one in the same statement that puts the new one in its place.
        let result = sqlx::query(
            "UPDATE webhook_subscriptions SET previous_sealed = secret_sealed, previous_until = $4, secret_sealed = $3 \
             WHERE id = $1 AND company_id = $2",
        )
        .bind(subscription.id().as_uuid())
        .bind(subscription.company_id().as_uuid())
        .bind(secret.as_bytes())
        .bind(previous_until)
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<Option<WebhookSubscription>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_SUBSCRIPTIONS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(subscription_from_row).transpose()
    }

    async fn secrets_of(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<Option<WebhookSecrets>, RepositoryError> {
        let row = sqlx::query(
            "SELECT secret_sealed, previous_sealed, previous_until FROM webhook_subscriptions \
             WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.map(|row| {
            let current: Vec<u8> = row.try_get("secret_sealed").map_err(storage_error)?;
            let previous: Option<Vec<u8>> =
                row.try_get("previous_sealed").map_err(storage_error)?;
            let until: Option<i64> = row.try_get("previous_until").map_err(storage_error)?;
            let sealed =
                |bytes: Vec<u8>| SealedSecret::new(bytes).map_err(|error| corrupt("secret", error));
            Ok(WebhookSecrets {
                current: sealed(current)?,
                previous: match (previous, until) {
                    (Some(bytes), Some(until)) => Some((sealed(bytes)?, until)),
                    _ => None,
                },
            })
        })
        .transpose()
    }

    async fn list(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<WebhookSubscription>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_SUBSCRIPTIONS} WHERE company_id = $1 ORDER BY created_at, id"
        ))
        .bind(company_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(subscription_from_row).collect()
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: WebhookSubscriptionId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::WebhookSubscription,
            id.as_uuid(),
        )
        .await
    }

    async fn publish(
        &self,
        company_id: CompanyId,
        event_id: Uuid,
        kind: WebhookEventKind,
        payload: &str,
        now: i64,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError> {
        let rows = sqlx::query(&format!(
            "{SELECT_SUBSCRIPTIONS} WHERE company_id = $1 AND active \
             AND (cardinality(events) = 0 OR $2 = ANY(events)) ORDER BY created_at, id"
        ))
        .bind(company_id.as_uuid())
        .bind(kind.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let mut made = Vec::new();
        for row in &rows {
            let subscription = subscription_from_row(row)?;
            if let Some(delivery) =
                insert_delivery(&self.pool, &subscription, event_id, kind, payload, now).await?
            {
                made.push(delivery);
            }
        }
        Ok(made)
    }

    async fn deliver_to(
        &self,
        subscription: &WebhookSubscription,
        event_id: Uuid,
        kind: WebhookEventKind,
        payload: &str,
        now: i64,
    ) -> Result<Option<WebhookDelivery>, RepositoryError> {
        insert_delivery(&self.pool, subscription, event_id, kind, payload, now).await
    }

    async fn find_delivery(
        &self,
        id: WebhookDeliveryId,
    ) -> Result<Option<WebhookDelivery>, RepositoryError> {
        let row = sqlx::query(&format!("{SELECT_DELIVERIES} WHERE id = $1"))
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?;
        row.as_ref().map(delivery_from_row).transpose()
    }

    async fn record_attempt(
        &self,
        id: WebhookDeliveryId,
        status: DeliveryStatus,
        attempts: u32,
        last_status: Option<u16>,
        last_error: Option<&str>,
        now: i64,
    ) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE webhook_deliveries SET status = $2, attempts = $3, last_status = $4, last_error = $5, \
                 updated_at = $6, delivered_at = CASE WHEN $2 = 'delivered' THEN $6 ELSE NULL END \
             WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(status.as_str())
        .bind(i32::try_from(attempts).unwrap_or(i32::MAX))
        .bind(last_status.map(i32::from))
        .bind(last_error)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn reset_delivery(
        &self,
        company_id: CompanyId,
        subscription_id: WebhookSubscriptionId,
        id: WebhookDeliveryId,
        now: i64,
    ) -> Result<Option<WebhookDelivery>, RepositoryError> {
        let row = sqlx::query(
            "UPDATE webhook_deliveries SET status = 'pending', attempts = 0, last_status = NULL, last_error = NULL, \
                 delivered_at = NULL, updated_at = $4 \
             WHERE id = $1 AND company_id = $2 AND subscription_id = $3 \
             RETURNING id, company_id, subscription_id, event_id, kind, payload, status, attempts, last_status, \
                 last_error, created_at, updated_at, delivered_at",
        )
        .bind(id.as_uuid())
        .bind(company_id.as_uuid())
        .bind(subscription_id.as_uuid())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(delivery_from_row).transpose()
    }

    async fn list_deliveries(
        &self,
        company_id: CompanyId,
        subscription_id: WebhookSubscriptionId,
        after: Option<&DeliveryCursor>,
        limit: usize,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError> {
        let (created_at, cursor_id) = after.map_or((i64::MAX, Uuid::max()), |cursor| {
            (cursor.created_at, cursor.id)
        });
        let rows = sqlx::query(&format!(
            "{SELECT_DELIVERIES} WHERE company_id = $1 AND subscription_id = $2 \
             AND (created_at, id) < ($3, $4) ORDER BY created_at DESC, id DESC LIMIT $5"
        ))
        .bind(company_id.as_uuid())
        .bind(subscription_id.as_uuid())
        .bind(created_at)
        .bind(cursor_id)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.iter().map(delivery_from_row).collect()
    }

    async fn prune_before(&self, before: i64) -> Result<u64, RepositoryError> {
        let result = sqlx::query("DELETE FROM webhook_deliveries WHERE created_at < $1")
            .bind(before)
            .execute(&self.pool)
            .await
            .map_err(storage_error)?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use conta_verde_application::WebhookService;

    use super::*;
    use crate::{
        Argon2SecretService, PostgresJobQueue, RingSecretCipher, contract, contract_integrations,
    };

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn runs_webhooks() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = WebhookService::new(
            PostgresWebhookRepository::new(pool.clone()),
            RingSecretCipher::from_hex(&"11".repeat(32)).unwrap(),
            PostgresJobQueue::new(pool),
            Argon2SecretService::new().unwrap(),
            false,
        );
        contract_integrations::runs_webhooks(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_keeps_the_secret_sealed_and_refuses_what_the_domain_would() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let service = WebhookService::new(
            PostgresWebhookRepository::new(pool.clone()),
            RingSecretCipher::from_hex(&"22".repeat(32)).unwrap(),
            PostgresJobQueue::new(pool.clone()),
            Argon2SecretService::new().unwrap(),
            false,
        );
        let draft = conta_verde_domain::WebhookSubscriptionDraft {
            url: "https://hooks.example.com/x".into(),
            events: vec![],
            active: true,
        };
        let made = service
            .create(company, &contract::actor(), draft, 0)
            .await
            .unwrap();
        let stored: Vec<u8> =
            sqlx::query_scalar("SELECT secret_sealed FROM webhook_subscriptions WHERE id = $1")
                .bind(made.subscription.id().as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        // Sealed: the plain secret is nowhere in the row.
        assert!(
            !stored
                .windows(made.secret.len())
                .any(|window| window == made.secret.as_bytes())
        );
        for (url, events) in [
            ("ftp://x.com/a", "{}"),
            ("https://a b/", "{}"),
            ("https://x.com/a", "{nope}"),
        ] {
            let outcome = sqlx::query(
                "INSERT INTO webhook_subscriptions (id, company_id, url, events, active, secret_sealed, created_by_id, created_by_name, created_at) \
                 VALUES ($1, $2, $3, CAST($4 AS TEXT[]), true, '\\x01', $1, 'Ana', 0)",
            )
            .bind(uuid::Uuid::new_v4())
            .bind(company.as_uuid())
            .bind(url)
            .bind(events)
            .execute(&pool)
            .await;
            assert!(outcome.is_err(), "{url} {events}");
        }
    }
}
