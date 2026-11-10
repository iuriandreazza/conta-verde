//! The company's API keys, managed from the web app by owners and admins ([ADR 0087]). The secret of a key is in the answer
//! that makes it and in no other.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, IssuedKey};
use conta_verde_domain::{ApiKey, ApiKeyDraft, ApiKeyId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::clock::now_millis;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::ApiJson;
use crate::{Ports, SharedState};

/// How long the old key lives on after a rotation, when the request does not say.
const DEFAULT_GRACE_HOURS: u32 = 24;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateRequest {
    name: String,
    /// `area:read` or `area:write`.
    scopes: Vec<String>,
    /// How long the key lives, in hours; absent for one that does not expire.
    expires_in_hours: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RotateRequest {
    /// How long the old key goes on working, 0 to 168 hours.
    grace_hours: Option<u32>,
}

#[derive(Serialize)]
pub(crate) struct ApiKeyResponse {
    id: Uuid,
    name: String,
    /// The start of the secret, to tell keys apart; not enough to use one.
    label: String,
    scopes: Vec<String>,
    /// `active`, `expired` or `revoked`.
    status: &'static str,
    /// Unix milliseconds; `null` for a key that does not expire.
    expires_at: Option<i64>,
    created_at: i64,
    created_by: String,
    /// The key this one replaced.
    rotated_from: Option<Uuid>,
    /// `YYYY-MM-DD`; `null` while it has not been used.
    last_used_on: Option<String>,
}

impl ApiKeyResponse {
    fn new(key: &ApiKey, now: i64) -> Self {
        let status = if key.is_revoked() {
            "revoked"
        } else if key.is_usable_at(now) {
            "active"
        } else {
            "expired"
        };
        Self {
            id: key.id().as_uuid(),
            name: key.name().to_owned(),
            label: key.label().to_owned(),
            scopes: key.scope_keys(),
            status,
            expires_at: key.expires_at(),
            created_at: key.created_at(),
            created_by: key.created_by().name().to_owned(),
            rotated_from: key.rotated_from().map(|id| id.as_uuid()),
            last_used_on: key.last_used_on().map(|day| day.to_string()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ApiKeyListResponse {
    items: Vec<ApiKeyResponse>,
}

/// A key just made: the only answer that carries its secret.
#[derive(Serialize)]
pub(crate) struct IssuedKeyResponse {
    key: ApiKeyResponse,
    secret: String,
}

impl IssuedKeyResponse {
    fn new(issued: IssuedKey, now: i64) -> Self {
        Self {
            key: ApiKeyResponse::new(&issued.key, now),
            secret: issued.secret,
        }
    }
}

// A string, parsed here: axum's own rejection for a malformed id is plain text.
fn parse_id(raw: &str) -> Result<ApiKeyId, ApiError> {
    Uuid::parse_str(raw)
        .map(ApiKeyId::from_uuid)
        .map_err(|_| ApplicationError::ApiKeyNotFound.into())
}

pub(crate) async fn list_api_keys<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<ApiKeyListResponse>, ApiError> {
    let now = now_millis();
    let keys = state.api_keys.list(company_id).await?;
    Ok(Json(ApiKeyListResponse {
        items: keys
            .iter()
            .map(|key| ApiKeyResponse::new(key, now))
            .collect(),
    }))
}

pub(crate) async fn create_api_key<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<CreateRequest>,
) -> Result<(StatusCode, Json<IssuedKeyResponse>), ApiError> {
    let now = now_millis();
    let issued = state
        .api_keys
        .create(
            company_id,
            &actor,
            ApiKeyDraft {
                name: request.name,
                scopes: request.scopes,
                expires_in_hours: request.expires_in_hours,
            },
            now,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(IssuedKeyResponse::new(issued, now)),
    ))
}

pub(crate) async fn rotate_api_key<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, key_id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<RotateRequest>,
) -> Result<(StatusCode, Json<IssuedKeyResponse>), ApiError> {
    let now = now_millis();
    let issued = state
        .api_keys
        .rotate(
            company_id,
            &actor,
            parse_id(&key_id)?,
            request.grace_hours.unwrap_or(DEFAULT_GRACE_HOURS),
            now,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(IssuedKeyResponse::new(issued, now)),
    ))
}

pub(crate) async fn revoke_api_key<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, key_id)): Path<(Uuid, String)>,
) -> Result<Json<ApiKeyResponse>, ApiError> {
    let key = state
        .api_keys
        .revoke(company_id, &actor, parse_id(&key_id)?)
        .await?;
    Ok(Json(ApiKeyResponse::new(&key, now_millis())))
}

pub(crate) async fn api_key_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, key_id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .api_keys
        .history(company_id, parse_id(&key_id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
