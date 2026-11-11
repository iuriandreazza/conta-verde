use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::HeaderMap;
use conta_verde_application::{ImportRow, MAX_STOCK_IMPORT_ROWS, StockImportDraft};
use serde::Deserialize;

use crate::access::CompanyAccess;
use crate::csv_import::{ImportResponse, Record, read_file, read_table};
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::products_import::decimal;
use crate::{Ports, SharedState};

const COLUMNS: &[&str] = &["sku", "quantity", "unit_cost"];
const REQUIRED: &[&str] = &["sku", "quantity"];

/// What the file is for: the day the balances are opened and the name the person gives this file, which is what
/// makes sending it twice add nothing ([ADR 0094]). The server reads no clock, so the day is the person's.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StockImportQuery {
    reference: String,
    on: String,
    dry_run: Option<bool>,
}

/// A row of the file as what a product holds. The columns have the names of the movement's fields; a quantity or a
/// cost may use the decimal comma of a Brazilian spreadsheet.
fn balance_from_record(record: &Record<'_>) -> Result<StockImportDraft, String> {
    let sku = record
        .get("sku")
        .ok_or_else(|| "sku is required".to_owned())?;
    let quantity = decimal("quantity", record.get("quantity"))?
        .ok_or_else(|| "quantity is required".to_owned())?;
    Ok(StockImportDraft {
        sku: sku.to_owned(),
        quantity,
        unit_cost: decimal("unit_cost", record.get("unit_cost"))?,
    })
}

/// Opens the stock of the products of a CSV file, or with `?dry_run=true` only says what would happen. Like every
/// import the whole file is checked before anything is written, and a row that a file with the same `reference`
/// already opened is skipped.
pub(crate) async fn import_stock_balances<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiQuery(query): ApiQuery<StockImportQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<ImportResponse>, ApiError> {
    let bytes = read_file(&headers, body).await?;
    let table = read_table(&bytes, COLUMNS, REQUIRED, MAX_STOCK_IMPORT_ROWS)?;
    let (rows, mut refused) = table.rows(balance_from_record);
    let dry_run = query.dry_run.unwrap_or(false);
    let report = state
        .stock
        .import_balances(
            company_id,
            &actor,
            &query.reference,
            &query.on,
            rows.into_iter()
                .map(|ImportRow { row, draft }| (row, draft))
                .collect(),
            dry_run || !refused.is_empty(),
        )
        .await?;
    refused.extend(report.refused.iter().cloned());
    Ok(Json(ImportResponse::new(dry_run, report, refused)))
}
