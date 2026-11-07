use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::HeaderMap;
use conta_verde_application::{
    EntryImportDraft, ImportRow, MAX_ENTRY_IMPORT_ROWS, SettlementImport,
};

use crate::access::CompanyAccess;
use crate::csv_import::{ImportQuery, ImportResponse, Record, read_file, read_table};
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

const COLUMNS: &[&str] = &[
    "reference",
    "kind",
    "description",
    "party_document",
    "party_name",
    "category",
    "cost_center",
    "competence_date",
    "due_date",
    "amount",
    "settled_on",
    "applied",
    "interest",
    "fine",
    "discount",
    "bank_account",
];
const REQUIRED: &[&str] = &[
    "reference",
    "kind",
    "description",
    "category",
    "competence_date",
    "due_date",
    "amount",
];
const SETTLEMENT_COLUMNS: [&str; 5] = ["applied", "interest", "fine", "discount", "bank_account"];

/// A row of the file as an entry with one installment, paid or open. The columns have the names of
/// the entry's fields; what the row points at is named, and the use case finds it ([ADR 0064]).
fn entry_from_record(record: &Record<'_>) -> Result<EntryImportDraft, String> {
    let required = |column: &str| {
        record
            .get(column)
            .map(str::to_owned)
            .ok_or_else(|| format!("{column} is required"))
    };
    let text = |column: &str| record.get(column).map(str::to_owned);
    let settlement = match record.get("settled_on") {
        Some(settled_on) => Some(SettlementImport {
            settled_on: settled_on.to_owned(),
            applied: text("applied"),
            interest: text("interest"),
            fine: text("fine"),
            discount: text("discount"),
            bank_account: required("bank_account")
                .map_err(|_| "bank_account is required when settled_on is given".to_owned())?,
        }),
        None => {
            if let Some(column) = SETTLEMENT_COLUMNS
                .iter()
                .find(|column| record.get(column).is_some())
            {
                return Err(format!("{column} is only for a row with settled_on"));
            }
            None
        }
    };
    Ok(EntryImportDraft {
        reference: required("reference")?,
        kind: required("kind")?,
        description: required("description")?,
        party_document: text("party_document"),
        party_name: text("party_name"),
        category: required("category")?,
        cost_center: text("cost_center"),
        competence_date: required("competence_date")?,
        due_date: required("due_date")?,
        amount: required("amount")?,
        settlement,
    })
}

/// Registers the entries of a CSV file, each with one installment and, when the row says it was paid,
/// its settlement, or with `?dry_run=true` only says what would happen. The whole file is checked before
/// anything is written: if a row is refused, nothing is. A row whose reference was imported before is
/// skipped, which makes sending a file again safe.
pub(crate) async fn import_entries<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiQuery(query): ApiQuery<ImportQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<ImportResponse>, ApiError> {
    let bytes = read_file(&headers, body).await?;
    let table = read_table(&bytes, COLUMNS, REQUIRED, MAX_ENTRY_IMPORT_ROWS)?;
    let (rows, mut refused) = table.rows(entry_from_record);

    // A row that could not even be read does not stop the others from being checked, so that the
    // person gets every problem of the file at once; but nothing is written.
    let dry_run = query.dry_run();
    let report = state
        .entries
        .import_entries(
            company_id,
            &actor,
            rows.into_iter()
                .map(|ImportRow { row, draft }| (row, draft))
                .collect(),
            dry_run || !refused.is_empty(),
        )
        .await?;
    refused.extend(report.refused.iter().cloned());
    Ok(Json(ImportResponse::new(dry_run, report, refused)))
}
