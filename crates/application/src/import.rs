use std::collections::HashMap;
use std::future::Future;

use conta_verde_domain::DomainError;

use crate::{ApplicationError, RepositoryError};

/// The most rows one import takes: a file is read and checked in memory, and its rows are written
/// one after the other, so a file past this is split by the person, not accepted slowly.
pub const MAX_IMPORT_ROWS: usize = 1000;

/// A row of an import file, with the number a spreadsheet shows for it (the header is row 1).
#[derive(Debug, Clone)]
pub struct ImportRow<T> {
    pub row: usize,
    pub draft: T,
}

/// Something to say about one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowNote {
    pub row: usize,
    pub message: String,
}

/// What an import did, or with `dry_run` would do.
///
/// Rows are checked all together before any is written: if one is `refused` nothing is created, so
/// the person fixes the file and sends it again. A row whose key (a document, a SKU) is already
/// registered is `skipped`, never overwritten, which is also what makes sending the same file twice
/// safe for the rows that have a key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Rows that passed every check, whether or not they were written.
    pub valid: usize,
    pub created: usize,
    pub skipped: Vec<RowNote>,
    pub refused: Vec<RowNote>,
}

/// What every import does with its rows, whatever the register: build each one as when registering
/// it by hand, refuse a key (`key_name`: a document, a SKU) that repeats within the file, and,
/// only if every row is fine and it is not a `dry_run`, save them one by one. A row that `save`
/// refuses as the `duplicate` error is skipped, never overwritten; any other failure stops the
/// import and the rows before it stay.
///
/// `save` returns a named future rather than being an async closure, which the compiler cannot
/// prove `Send` for a handler that runs on a multi-threaded runtime.
pub(crate) async fn import_rows<D, T, Saving>(
    rows: Vec<ImportRow<D>>,
    dry_run: bool,
    key_name: &str,
    duplicate: RepositoryError,
    build: impl Fn(D) -> Result<T, DomainError> + Send,
    key: impl Fn(&T) -> Option<String> + Send,
    save: impl Fn(T) -> Saving + Send,
) -> Result<ImportReport, ApplicationError>
where
    D: Send,
    T: Send,
    Saving: Future<Output = Result<(), RepositoryError>> + Send,
{
    let mut report = ImportReport::default();
    let mut built = Vec::with_capacity(rows.len());
    let mut first_row_of: HashMap<String, usize> = HashMap::new();
    for ImportRow { row, draft } in rows {
        let record = match build(draft) {
            Ok(record) => record,
            Err(error) => {
                report.refused.push(RowNote {
                    row,
                    message: error.to_string(),
                });
                continue;
            }
        };
        if let Some(key) = key(&record)
            && let Some(first) = first_row_of.insert(key, row)
        {
            report.refused.push(RowNote {
                row,
                message: format!("the same {key_name} as row {first}"),
            });
            continue;
        }
        built.push((row, record));
    }
    report.valid = built.len();
    if dry_run || !report.refused.is_empty() {
        return Ok(report);
    }

    for (row, record) in built {
        match save(record).await {
            Ok(()) => report.created += 1,
            Err(error) if error == duplicate => report.skipped.push(RowNote {
                row,
                message: error.to_string(),
            }),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(report)
}
