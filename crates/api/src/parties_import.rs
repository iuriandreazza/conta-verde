use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::HeaderMap;
use conta_verde_domain::{AddressDraft, PartyDraft};

use crate::access::CompanyAccess;
use crate::csv_import::{
    FileError, ImportQuery, ImportResponse, Record, read_file, read_table, yes_or_no,
};
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

const COLUMNS: &[&str] = &[
    "name",
    "document",
    "email",
    "customer",
    "supplier",
    "carrier",
    "seller",
    "postal_code",
    "street",
    "number",
    "complement",
    "district",
    "city",
    "state",
    "tags",
];
const ROLES: [&str; 4] = ["customer", "supplier", "carrier", "seller"];
const ADDRESS_COLUMNS: [&str; 7] = [
    "postal_code",
    "street",
    "number",
    "complement",
    "district",
    "city",
    "state",
];
/// Tags share a cell, and a comma or a semicolon would clash with the file's own separator.
const TAG_SEPARATOR: char = '|';

/// A party as one row of the file. The columns have the names of the party's fields; the roles
/// are `sim` or `não` (blank is no) and the tags are separated by `|`. Contacts have no column:
/// several people per party do not fit a row, and are added by hand.
fn party_from_record(record: &Record<'_>) -> Result<PartyDraft, String> {
    let text = |column: &str| record.get(column).map(str::to_owned);
    let role = |column: &str| {
        yes_or_no(record.get(column)).ok_or_else(|| format!("{column} must be sim or não"))
    };
    let address = ADDRESS_COLUMNS
        .iter()
        .any(|column| record.get(column).is_some())
        .then(|| AddressDraft {
            postal_code: text("postal_code").unwrap_or_default(),
            street: text("street").unwrap_or_default(),
            number: text("number"),
            complement: text("complement"),
            district: text("district"),
            city: text("city").unwrap_or_default(),
            state: text("state").unwrap_or_default(),
        });
    Ok(PartyDraft {
        name: text("name").unwrap_or_default(),
        document: text("document"),
        email: text("email"),
        is_customer: role("customer")?,
        is_supplier: role("supplier")?,
        is_carrier: role("carrier")?,
        is_seller: role("seller")?,
        address,
        contacts: Vec::new(),
        tags: record
            .get("tags")
            .map(|tags| tags.split(TAG_SEPARATOR).map(str::to_owned).collect())
            .unwrap_or_default(),
    })
}

/// Registers the parties of a CSV file, or with `?dry_run=true` only says what would happen. The
/// whole file is checked before anything is written: if a row is refused, nothing is. A row whose
/// document is already registered is skipped, not overwritten. The registry is not asked about the
/// CNPJs of the file (ADR 0025).
pub(crate) async fn import_parties<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiQuery(query): ApiQuery<ImportQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<ImportResponse>, ApiError> {
    let bytes = read_file(&headers, body).await?;
    let table = read_table(&bytes, COLUMNS, &["name"])?;
    if !ROLES.iter().any(|role| table.has_column(role)) {
        return Err(FileError::MissingRoleColumn.into());
    }
    let (rows, mut refused) = table.rows(party_from_record);

    // A row that could not even be read does not stop the others from being checked, so that the
    // person gets every problem of the file at once; but nothing is written.
    let dry_run = query.dry_run();
    let report = state
        .parties
        .import(company_id, &actor, rows, dry_run || !refused.is_empty())
        .await?;
    refused.extend(report.refused.iter().cloned());
    Ok(Json(ImportResponse::new(dry_run, report, refused)))
}
