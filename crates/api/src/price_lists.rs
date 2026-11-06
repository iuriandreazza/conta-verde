use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, PriceListQuery};
use conta_verde_domain::{PriceList, PriceListDraft, PriceListId, PriceListItemDraft, ProductId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_cursor, encode_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery, field_or};
use crate::list_params;
use crate::{Ports, SharedState};

/// One price: a product and its unit price, a string like every decimal of this API.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PriceRequest {
    product_id: Uuid,
    price: String,
}

/// The body of both registering and changing a price list: a change replaces the name, the flag
/// and every price.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PriceListRequest {
    name: String,
    /// At least one and at most 1000, each product once.
    items: Vec<PriceRequest>,
    /// Optional when registering (active), required when replacing: a body without it would
    /// otherwise switch a deactivated price list back on.
    active: Option<bool>,
}

impl PriceListRequest {
    /// `default_active` is what a missing `active` means: `Some(true)` when registering, `None`
    /// when replacing, where it must be said.
    fn into_draft(self, default_active: Option<bool>) -> Result<PriceListDraft, ApiError> {
        let active = field_or(
            self.active,
            default_active,
            "active is required when replacing a price list",
        )?;
        Ok(PriceListDraft {
            name: self.name,
            items: self
                .items
                .into_iter()
                .map(|item| PriceListItemDraft {
                    product_id: ProductId::from_uuid(item.product_id),
                    price: item.price,
                })
                .collect(),
            active,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the name.
    q: Option<String>,
    /// `true` or `false`; both are listed when it is absent.
    active: Option<String>,
    /// `name` (default) or `-name` for descending.
    sort: Option<String>,
}

impl ListQuery {
    fn price_list_query(&self) -> Result<PriceListQuery, ApiError> {
        let mut query = PriceListQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(active) = list_params::active(self.active.as_deref())? {
            query = query.with_active(active);
        }
        Ok(query.sorted(list_params::sort(self.sort.as_deref())?))
    }
}

#[derive(Serialize)]
pub(crate) struct PriceResponse {
    product_id: Uuid,
    /// A decimal string without trailing zeros, like `"10.5"`.
    price: String,
}

#[derive(Serialize)]
pub(crate) struct PriceListResponse {
    id: Uuid,
    company_id: Uuid,
    name: String,
    items: Vec<PriceResponse>,
    active: bool,
}

impl From<&PriceList> for PriceListResponse {
    fn from(list: &PriceList) -> Self {
        Self {
            id: list.id().as_uuid(),
            company_id: list.company_id().as_uuid(),
            name: list.name().to_owned(),
            items: list
                .items()
                .iter()
                .map(|item| PriceResponse {
                    product_id: item.product_id().as_uuid(),
                    price: item.price().to_string(),
                })
                .collect(),
            active: list.is_active(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct PriceListPageResponse {
    items: Vec<PriceListResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<PriceListId, ApiError> {
    Uuid::parse_str(raw)
        .map(PriceListId::from_uuid)
        .map_err(|_| ApplicationError::PriceListNotFound.into())
}

pub(crate) async fn list_price_lists<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<PriceListPageResponse>, ApiError> {
    let filter = query.price_list_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .price_lists
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(PriceListPageResponse {
        items: page.items.iter().map(PriceListResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn register_price_list<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<PriceListRequest>,
) -> Result<(StatusCode, Json<PriceListResponse>), ApiError> {
    let list = state
        .price_lists
        .register(company_id, &actor, request.into_draft(Some(true))?)
        .await?;
    Ok((StatusCode::CREATED, Json(PriceListResponse::from(&list))))
}

/// Replaces the name, the flag and every price. Nothing is written, and nothing is
/// recorded, when nothing changes.
pub(crate) async fn update_price_list<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<PriceListRequest>,
) -> Result<Json<PriceListResponse>, ApiError> {
    let list = state
        .price_lists
        .update(
            company_id,
            &actor,
            parse_id(&id)?,
            request.into_draft(None)?,
        )
        .await?;
    Ok(Json(PriceListResponse::from(&list)))
}

/// What was recorded about a price list, oldest first.
pub(crate) async fn price_list_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .price_lists
        .history(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
