use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::HeaderMap;
use conta_verde_domain::{ProductDraft, ProductKind};

use crate::access::CompanyAccess;
use crate::csv_import::{ImportQuery, ImportResponse, Record, read_file, read_table, yes_or_no};
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

const COLUMNS: &[&str] = &[
    "kind",
    "name",
    "sku",
    "unit",
    "ncm",
    "sale_price",
    "cost_price",
    "minimum_stock",
    "active",
];

/// `product` or `service`, or the same in Portuguese; blank is a product, which most rows are.
fn kind_of(cell: Option<&str>) -> Result<ProductKind, String> {
    match cell.map(str::to_lowercase).as_deref() {
        None | Some("product" | "produto") => Ok(ProductKind::Product),
        Some("service" | "serviço" | "servico") => Ok(ProductKind::Service),
        Some(_) => Err("kind must be product or service".to_owned()),
    }
}

/// A price as a spreadsheet writes it in Brazil, `12,50`, becomes `12.50`. Only that: a cell with
/// both a comma and a dot (`1.234,56`) is left as it is and refused by the amount's own strict
/// parsing, since guessing which one is the thousands separator could change a price. A comma
/// followed by exactly three digits (`1,234`) is ambiguous the same way, a thousand in a spreadsheet
/// from another locale and a decimal in this one, so it is left alone too and refused: the person
/// writes `1234` or `1,2340`, whichever is meant.
fn decimal(cell: Option<&str>) -> Option<String> {
    cell.map(|text| {
        let ambiguous = text.split_once(',').is_some_and(|(_, after)| {
            after.len() == 3 && after.bytes().all(|b| b.is_ascii_digit())
        });
        if text.contains(',') && !text.contains('.') && !ambiguous {
            text.replace(',', ".")
        } else {
            text.to_owned()
        }
    })
}

/// A product as one row of the file. The columns have the names of the product's fields; `kind`
/// and `active` may be left out (a product, active), and so may the prices and the stock.
fn product_from_record(record: &Record<'_>) -> Result<ProductDraft, String> {
    let text = |column: &str| record.get(column).map(str::to_owned);
    let active = match record.get("active") {
        None => true,
        cell => yes_or_no(cell).ok_or("active must be sim or não")?,
    };
    Ok(ProductDraft {
        kind: kind_of(record.get("kind"))?,
        name: text("name").unwrap_or_default(),
        sku: text("sku"),
        unit: text("unit").unwrap_or_default(),
        ncm: text("ncm"),
        sale_price: decimal(record.get("sale_price")),
        cost_price: decimal(record.get("cost_price")),
        minimum_stock: decimal(record.get("minimum_stock")),
        active,
    })
}

/// Registers the products of a CSV file, or with `?dry_run=true` only says what would happen,
/// under the rules of every import (ADR 0025): the whole file is checked first and nothing is
/// written if a row is refused; a SKU already registered is skipped, never overwritten.
pub(crate) async fn import_products<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiQuery(query): ApiQuery<ImportQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<ImportResponse>, ApiError> {
    let bytes = read_file(&headers, body).await?;
    let (rows, mut refused) =
        read_table(&bytes, COLUMNS, &["name", "unit"])?.rows(product_from_record);

    let dry_run = query.dry_run();
    let report = state
        .products
        .import(company_id, &actor, rows, dry_run || !refused.is_empty())
        .await?;
    refused.extend(report.refused.iter().cloned());
    Ok(Json(ImportResponse::new(dry_run, report, refused)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decimal_comma_becomes_a_dot_and_nothing_ambiguous_is_guessed() {
        let read = |text: &str| decimal(Some(text)).unwrap();
        assert_eq!(read("12,50"), "12.50");
        assert_eq!(read("5,5"), "5.5");
        assert_eq!(read("12.50"), "12.50");
        assert_eq!(read("1500"), "1500");
        // These are refused by the amount's strict parsing, never guessed at.
        for ambiguous in ["1.234,56", "1,234", "1,234,567", "12,5,0", "1,500"] {
            assert!(
                conta_verde_domain::Amount::parse(&read(ambiguous)).is_none(),
                "{ambiguous}"
            );
        }
        assert!(conta_verde_domain::Amount::parse(&read("1,2340")).is_some());
        assert_eq!(decimal(None), None);
    }
}
