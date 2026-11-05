use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, MAX_SEARCH_CHARS, NameSort, ProductQuery};
use conta_verde_domain::{Product, ProductDraft, ProductId, ProductKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::cursor::{decode_cursor, encode_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::{Ports, SharedState};

/// The body of both registering and changing a product: a change replaces every editable field.
/// Prices and the minimum stock are strings, never JSON numbers, so no precision is lost.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProductRequest {
    kind: KindParameter,
    name: String,
    sku: Option<String>,
    unit: String,
    ncm: Option<String>,
    sale_price: Option<String>,
    cost_price: Option<String>,
    minimum_stock: Option<String>,
    /// Optional when registering (active), required when replacing: a body without it would
    /// otherwise switch a deactivated product back on.
    active: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum KindParameter {
    Product,
    Service,
}

impl From<KindParameter> for ProductKind {
    fn from(kind: KindParameter) -> Self {
        match kind {
            KindParameter::Product => Self::Product,
            KindParameter::Service => Self::Service,
        }
    }
}

impl ProductRequest {
    /// `default_active` is what a missing `active` means: `Some(true)` when registering, `None`
    /// when replacing, where it must be said.
    fn into_draft(self, default_active: Option<bool>) -> Result<ProductDraft, ApiError> {
        let active = self
            .active
            .or(default_active)
            .ok_or(ApiError::InvalidField(
                "active is required when replacing a product",
            ))?;
        Ok(ProductDraft {
            active,
            ..ProductDraft::from(self)
        })
    }
}

impl From<ProductRequest> for ProductDraft {
    fn from(request: ProductRequest) -> Self {
        Self {
            kind: request.kind.into(),
            name: request.name,
            sku: request.sku,
            unit: request.unit,
            ncm: request.ncm,
            sale_price: request.sale_price,
            cost_price: request.cost_price,
            minimum_stock: request.minimum_stock,
            active: request.active.unwrap_or(true),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the name or the SKU.
    q: Option<String>,
    /// `product` or `service`.
    kind: Option<String>,
    /// `true` or `false`; both are listed when it is absent.
    active: Option<String>,
    /// `name` (default) or `-name` for descending.
    sort: Option<String>,
}

impl ListQuery {
    fn product_query(&self) -> Result<ProductQuery, ApiError> {
        let mut query = ProductQuery::default();
        if let Some(text) = &self.q {
            if text.chars().count() > MAX_SEARCH_CHARS {
                return Err(ApiError::InvalidQuery("invalid q: at most 100 characters"));
            }
            query = query.searching(text);
        }
        query = match self.kind.as_deref() {
            None => query,
            Some("product") => query.of_kind(ProductKind::Product),
            Some("service") => query.of_kind(ProductKind::Service),
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid kind: expected product or service",
                ));
            }
        };
        query = match self.active.as_deref() {
            None => query,
            Some("true") => query.with_active(true),
            Some("false") => query.with_active(false),
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid active: expected true or false",
                ));
            }
        };
        Ok(match self.sort.as_deref() {
            None | Some("name") => query,
            Some("-name") => query.sorted(NameSort::NameDesc),
            Some(_) => {
                return Err(ApiError::InvalidQuery(
                    "invalid sort: expected name or -name",
                ));
            }
        })
    }
}

#[derive(Serialize)]
pub(crate) struct ProductResponse {
    id: Uuid,
    company_id: Uuid,
    kind: &'static str,
    name: String,
    sku: Option<String>,
    unit: String,
    ncm: Option<String>,
    /// Decimal strings without trailing zeros, like `"12.5"`.
    sale_price: Option<String>,
    cost_price: Option<String>,
    minimum_stock: Option<String>,
    active: bool,
}

impl From<&Product> for ProductResponse {
    fn from(product: &Product) -> Self {
        Self {
            id: product.id().as_uuid(),
            company_id: product.company_id().as_uuid(),
            kind: product.kind().as_str(),
            name: product.name().to_owned(),
            sku: product.sku().map(str::to_owned),
            unit: product.unit().to_owned(),
            ncm: product.ncm().map(str::to_owned),
            sale_price: product.sale_price().map(|amount| amount.to_string()),
            cost_price: product.cost_price().map(|amount| amount.to_string()),
            minimum_stock: product.minimum_stock().map(|amount| amount.to_string()),
            active: product.is_active(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ProductPageResponse {
    items: Vec<ProductResponse>,
    next_cursor: Option<String>,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_product_id(raw: &str) -> Result<ProductId, ApiError> {
    Uuid::parse_str(raw)
        .map(ProductId::from_uuid)
        .map_err(|_| ApplicationError::ProductNotFound.into())
}

pub(crate) async fn list_products<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<ProductPageResponse>, ApiError> {
    let filter = query.product_query()?;
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .products
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(ProductPageResponse {
        items: page.items.iter().map(ProductResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn register_product<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<ProductRequest>,
) -> Result<(StatusCode, Json<ProductResponse>), ApiError> {
    let product = state
        .products
        .register(company_id, &actor, request.into_draft(Some(true))?)
        .await?;
    Ok((StatusCode::CREATED, Json(ProductResponse::from(&product))))
}

/// Replaces every editable field. Nothing is written, and nothing is recorded, when nothing changes.
pub(crate) async fn update_product<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, product_id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<ProductRequest>,
) -> Result<Json<ProductResponse>, ApiError> {
    let product_id = parse_product_id(&product_id)?;
    let product = state
        .products
        .update(company_id, &actor, product_id, request.into_draft(None)?)
        .await?;
    Ok(Json(ProductResponse::from(&product)))
}

/// What was recorded about a product, oldest first.
pub(crate) async fn product_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, product_id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let product_id = parse_product_id(&product_id)?;
    let history = state.products.history(company_id, product_id).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
