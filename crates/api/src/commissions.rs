use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use conta_verde_application::{
    ApplicationError, CommissionRuleQuery, CommissionStatement, RuleCursor,
};
use conta_verde_domain::{
    CalendarDate, CommissionRule, CommissionRuleDraft, CommissionRuleId, PartyId, ProductId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery, field_or};
use crate::list_params;
use crate::{Ports, SharedState};

/// The body of both registering and changing a rule: a change replaces every field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuleRequest {
    /// Left out is every seller.
    seller_id: Option<Uuid>,
    /// Left out is every product.
    product_id: Option<Uuid>,
    /// A percentage above 0 and at most 100, with at most two decimals, like `5.5`.
    rate: String,
    /// `YYYY-MM-DD`: the first day of sale it covers.
    starts_on: String,
    /// `YYYY-MM-DD`: the last day of sale it covers, or none.
    ends_on: Option<String>,
    /// Optional when registering (active), required when replacing.
    active: Option<bool>,
}

impl RuleRequest {
    fn into_draft(self, default_active: Option<bool>) -> Result<CommissionRuleDraft, ApiError> {
        Ok(CommissionRuleDraft {
            seller_id: self.seller_id.map(PartyId::from_uuid),
            product_id: self.product_id.map(ProductId::from_uuid),
            rate: self.rate,
            starts_on: self.starts_on,
            ends_on: self.ends_on,
            active: field_or(
                self.active,
                default_active,
                "active is required when replacing a rule",
            )?,
        })
    }
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    seller: Option<Uuid>,
    product: Option<Uuid>,
    /// `true` or `false`; both are listed when it is absent.
    active: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct RuleResponse {
    id: Uuid,
    company_id: Uuid,
    seller_id: Option<Uuid>,
    product_id: Option<Uuid>,
    rate: String,
    starts_on: String,
    ends_on: Option<String>,
    active: bool,
}

impl From<&CommissionRule> for RuleResponse {
    fn from(rule: &CommissionRule) -> Self {
        Self {
            id: rule.id().as_uuid(),
            company_id: rule.company_id().as_uuid(),
            seller_id: rule.seller_id().map(|id| id.as_uuid()),
            product_id: rule.product_id().map(|id| id.as_uuid()),
            rate: rule.rate().to_string(),
            starts_on: rule.starts_on().to_string(),
            ends_on: rule.ends_on().map(|day| day.to_string()),
            active: rule.is_active(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct RulePageResponse {
    items: Vec<RuleResponse>,
    next_cursor: Option<String>,
}

// The cursor is the day and the id, written like a date cursor.
fn encode(cursor: &RuleCursor) -> String {
    format!("{}|{}", cursor.id, cursor.starts_on)
}

fn decode(raw: &str) -> Result<RuleCursor, ApiError> {
    let (id, day) = raw.split_once('|').ok_or(ApiError::InvalidCursor)?;
    Ok(RuleCursor {
        id: Uuid::parse_str(id).map_err(|_| ApiError::InvalidCursor)?,
        starts_on: CalendarDate::parse(day).ok_or(ApiError::InvalidCursor)?,
    })
}

fn parse_id(raw: &str) -> Result<CommissionRuleId, ApiError> {
    Uuid::parse_str(raw)
        .map(CommissionRuleId::from_uuid)
        .map_err(|_| ApplicationError::CommissionRuleNotFound.into())
}

pub(crate) async fn list_rules<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<RulePageResponse>, ApiError> {
    let mut filter = CommissionRuleQuery::default();
    if let Some(seller) = query.seller {
        filter = filter.with_seller(PartyId::from_uuid(seller));
    }
    if let Some(product) = query.product {
        filter = filter.with_product(ProductId::from_uuid(product));
    }
    if let Some(active) = list_params::active(query.active.as_deref())? {
        filter = filter.with_active(active);
    }
    let after = query.after.as_deref().map(decode).transpose()?;
    let page = state
        .commissions
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(RulePageResponse {
        items: page.items.iter().map(RuleResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode),
    }))
}

pub(crate) async fn register_rule<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<RuleRequest>,
) -> Result<(StatusCode, Json<RuleResponse>), ApiError> {
    let rule = state
        .commissions
        .register(company_id, &actor, request.into_draft(Some(true))?)
        .await?;
    Ok((StatusCode::CREATED, Json(RuleResponse::from(&rule))))
}

/// Replaces every editable field. Nothing is written, and nothing is recorded, when nothing changes.
pub(crate) async fn update_rule<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<RuleRequest>,
) -> Result<Json<RuleResponse>, ApiError> {
    let rule = state
        .commissions
        .update(
            company_id,
            &actor,
            parse_id(&id)?,
            request.into_draft(None)?,
        )
        .await?;
    Ok(Json(RuleResponse::from(&rule)))
}

/// What was recorded about a rule, oldest first.
pub(crate) async fn rule_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .commissions
        .history(company_id, parse_id(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

#[derive(Deserialize)]
pub(crate) struct StatementQuery {
    /// `YYYY-MM-DD`: the first day of sale.
    from: String,
    /// `YYYY-MM-DD`: the last day of sale.
    to: String,
    seller: Option<Uuid>,
}

#[derive(Serialize)]
pub(crate) struct StatementSaleResponse {
    sale_id: Uuid,
    number: i64,
    sold_on: String,
    customer_id: Uuid,
    /// The net value of the lines, less what came back.
    base: String,
    commission: String,
}

#[derive(Serialize)]
pub(crate) struct SellerStatementResponse {
    seller_id: Uuid,
    base: String,
    commission: String,
    sales: Vec<StatementSaleResponse>,
}

#[derive(Serialize)]
pub(crate) struct StatementResponse {
    from: String,
    to: String,
    base: String,
    commission: String,
    sellers: Vec<SellerStatementResponse>,
}

impl From<&CommissionStatement> for StatementResponse {
    fn from(statement: &CommissionStatement) -> Self {
        Self {
            from: statement.from.to_string(),
            to: statement.to.to_string(),
            base: statement.base.to_string(),
            commission: statement.commission.to_string(),
            sellers: statement
                .sellers
                .iter()
                .map(|seller| SellerStatementResponse {
                    seller_id: seller.seller_id.as_uuid(),
                    base: seller.base.to_string(),
                    commission: seller.commission.to_string(),
                    sales: seller
                        .sales
                        .iter()
                        .map(|row| StatementSaleResponse {
                            sale_id: row.sale.id().as_uuid(),
                            number: row.sale.number(),
                            sold_on: row.sale.sold_on().to_string(),
                            customer_id: row.sale.customer_id().as_uuid(),
                            base: row.base.to_string(),
                            commission: row.commission.to_string(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// What the sellers earned on the completed sales of a span, by the rules in force on each sale's day.
pub(crate) async fn commission_statement<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<StatementQuery>,
) -> Result<Json<StatementResponse>, ApiError> {
    let from = CalendarDate::parse(&query.from)
        .ok_or(ApiError::InvalidQuery("invalid from: expected YYYY-MM-DD"))?;
    let to = CalendarDate::parse(&query.to)
        .ok_or(ApiError::InvalidQuery("invalid to: expected YYYY-MM-DD"))?;
    let statement = state
        .commissions
        .statement(company_id, query.seller.map(PartyId::from_uuid), from, to)
        .await?;
    Ok(Json(StatementResponse::from(&statement)))
}
