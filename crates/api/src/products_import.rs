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
        // The last is "serviço" with the cedilla written apart, as macOS saves it.
        Some("service" | "serviço" | "servico" | "servic\u{327}o") => Ok(ProductKind::Service),
        Some(_) => Err("kind must be product or service".to_owned()),
    }
}

/// A price or a quantity as a spreadsheet writes it in Brazil, `12,50`, becomes `12.50`. Only that
/// is converted, and a cell that could mean two amounts is refused instead of guessed:
///
/// - both a comma and a dot (`1.234,56`) is left as it is for the amount's strict parsing to refuse;
/// - one separator followed by exactly three digits, after one to three digits that do not start
///   with a zero (`1.234`, `1,234`), is a thousand in one locale and a decimal in another. `0,250`
///   cannot be a thousand, so it is a decimal. The person writes `1234`, or `1,2340` for the other.
fn decimal(column: &str, cell: Option<&str>) -> Result<Option<String>, String> {
    let Some(text) = cell else { return Ok(None) };
    let ambiguous = ['.', ','].iter().any(|separator| {
        text.matches(*separator).count() == 1
            && text.split_once(*separator).is_some_and(|(before, after)| {
                after.len() == 3
                    && after.bytes().all(|b| b.is_ascii_digit())
                    && (1..=3).contains(&before.len())
                    && !before.starts_with('0')
                    && before.bytes().all(|b| b.is_ascii_digit())
            })
    });
    if ambiguous {
        return Err(format!(
            "{column} `{text}` is ambiguous, a thousand or a decimal: write 1234, or {text}0 for the other"
        ));
    }
    Ok(Some(if text.contains(',') && !text.contains('.') {
        text.replace(',', ".")
    } else {
        text.to_owned()
    }))
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
        sale_price: decimal("sale_price", record.get("sale_price"))?,
        cost_price: decimal("cost_price", record.get("cost_price"))?,
        minimum_stock: decimal("minimum_stock", record.get("minimum_stock"))?,
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
        let read = |text: &str| decimal("sale_price", Some(text));
        assert_eq!(read("12,50"), Ok(Some("12.50".into())));
        assert_eq!(read("5,5"), Ok(Some("5.5".into())));
        assert_eq!(read("12.50"), Ok(Some("12.50".into())));
        assert_eq!(read("1500"), Ok(Some("1500".into())));
        // A zero in front cannot be a thousand, so it is a decimal.
        assert_eq!(read("0,250"), Ok(Some("0.250".into())));
        assert_eq!(read("0.250"), Ok(Some("0.250".into())));
        assert_eq!(read("1,2340"), Ok(Some("1.2340".into())));
        // A thousand or a decimal: refused, said so, never guessed.
        for ambiguous in ["1.234", "1,234", "12.500", "999,999"] {
            let error = read(ambiguous).unwrap_err();
            assert!(error.contains("ambiguous"), "{ambiguous}: {error}");
        }
        // These go on to the amount's strict parsing, which refuses them.
        for refused in ["1.234,56", "1,234,567", "12,5,0", "1.234.567"] {
            let text = read(refused).unwrap().unwrap();
            assert!(
                conta_verde_domain::Amount::parse(&text).is_none(),
                "{refused}"
            );
        }
        assert_eq!(decimal("sale_price", None), Ok(None));
    }
}
