use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{ApplicationError, NewCategory};
use conta_verde_domain::{CashFlowGroup, Category, CategoryId, DreGroup};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::list_params;
use crate::{Ports, SharedState};

/// A new category. A top-level one chooses `dre_group` and `cash_flow_group`; a subcategory names
/// its `parent_id` and nothing else, since it takes both groups from its parent.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateCategoryRequest {
    name: String,
    parent_id: Option<String>,
    dre_group: Option<String>,
    cash_flow_group: Option<String>,
}

impl CreateCategoryRequest {
    fn into_new_category(self) -> Result<NewCategory, ApiError> {
        let parent_id = self
            .parent_id
            .map(|raw| {
                Uuid::parse_str(&raw)
                    .map(CategoryId::from_uuid)
                    .map_err(|_| ApiError::InvalidField("invalid parent_id"))
            })
            .transpose()?;
        let dre_group = self
            .dre_group
            .map(|raw| DreGroup::parse(&raw).ok_or(ApiError::InvalidField("invalid dre_group")))
            .transpose()?;
        let cash_flow_group = self
            .cash_flow_group
            .map(|raw| {
                CashFlowGroup::parse(&raw).ok_or(ApiError::InvalidField("invalid cash_flow_group"))
            })
            .transpose()?;
        Ok(NewCategory {
            parent_id,
            name: self.name,
            dre_group,
            cash_flow_group,
        })
    }
}

/// What can change about a category: its name and whether it is active. Its parent and groups
/// are fixed, so a body that sends them is refused.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeCategoryRequest {
    name: String,
    active: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    /// `true` or `false`; both are listed when it is absent.
    active: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct CategoryResponse {
    id: Uuid,
    company_id: Uuid,
    /// `null` for a top-level category.
    parent_id: Option<Uuid>,
    name: String,
    /// Where the category lands in the DRE: `gross_revenue`, `deductions`, `costs`,
    /// `operating_expenses`, `financial_result`, `investments`, `outside_result` or `transfers`.
    dre_group: &'static str,
    /// `operating`, `investing`, `financing` or `transfer`.
    cash_flow_group: &'static str,
    /// 1 for a top-level category.
    level: u8,
    active: bool,
}

impl From<&Category> for CategoryResponse {
    fn from(category: &Category) -> Self {
        Self {
            id: category.id().as_uuid(),
            company_id: category.company_id().as_uuid(),
            parent_id: category.parent_id().map(|parent| parent.as_uuid()),
            name: category.name().to_owned(),
            dre_group: category.dre_group().as_str(),
            cash_flow_group: category.cash_flow_group().as_str(),
            level: category.level(),
            active: category.is_active(),
        }
    }
}

/// The whole tree as a flat list: a tree is drawn whole, so this is not paged (ADR 0012).
#[derive(Serialize)]
pub(crate) struct CategoryListResponse {
    items: Vec<CategoryResponse>,
}

impl CategoryListResponse {
    fn of(categories: &[Category]) -> Self {
        Self {
            items: categories.iter().map(CategoryResponse::from).collect(),
        }
    }
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<CategoryId, ApiError> {
    Uuid::parse_str(raw)
        .map(CategoryId::from_uuid)
        .map_err(|_| ApplicationError::CategoryNotFound.into())
}

pub(crate) async fn list_categories<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<CategoryListResponse>, ApiError> {
    let active = list_params::active(query.active.as_deref())?;
    let categories = state.categories.list(company_id, active).await?;
    Ok(Json(CategoryListResponse::of(&categories)))
}

pub(crate) async fn create_category<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<CategoryResponse>), ApiError> {
    let category = state
        .categories
        .create(company_id, &actor, request.into_new_category()?)
        .await?;
    Ok((StatusCode::CREATED, Json(CategoryResponse::from(&category))))
}

/// Gives a company without categories the usual ones, in one write.
pub(crate) async fn apply_default_template<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
) -> Result<(StatusCode, Json<CategoryListResponse>), ApiError> {
    let categories = state
        .categories
        .apply_default_template(company_id, &actor)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CategoryListResponse::of(&categories)),
    ))
}

/// Renames a category or switches it on or off. Nothing is written, and nothing is recorded, when
/// nothing changes.
pub(crate) async fn change_category<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<ChangeCategoryRequest>,
) -> Result<Json<CategoryResponse>, ApiError> {
    let active = request.active.ok_or(ApiError::InvalidField(
        "active is required when changing a category",
    ))?;
    let category = state
        .categories
        .update(company_id, &actor, parse_id(&id)?, &request.name, active)
        .await?;
    Ok(Json(CategoryResponse::from(&category)))
}

/// What was recorded about a category, oldest first.
pub(crate) async fn category_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.categories.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}
