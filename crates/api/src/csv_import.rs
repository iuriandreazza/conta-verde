//! Reading the CSV file of an import into rows. What a row means is each register's business; this
//! module only turns text into a table with named columns, and says plainly what is wrong with a
//! file that cannot be read at all. Everything that is wrong with a single row is that row's.

use std::collections::HashSet;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, header};
use conta_verde_application::{ImportReport, ImportRow, MAX_IMPORT_ROWS, RowNote};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

/// The largest file accepted: 1000 rows of anything an ERP register holds fit in a fraction of it.
pub(crate) const MAX_FILE_BYTES: usize = 1024 * 1024;

/// The query of every import endpoint.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportQuery {
    /// Check the file and say what would happen, without writing anything.
    dry_run: Option<bool>,
}

impl ImportQuery {
    pub(crate) fn dry_run(&self) -> bool {
        self.dry_run.unwrap_or(false)
    }
}

/// The file of the request, at most [`MAX_FILE_BYTES`]. A `Content-Length` past it is refused before
/// the body is read, so a browser that is still uploading gets a clean 413, not a dropped line.
pub(crate) async fn read_file(headers: &HeaderMap, body: Body) -> Result<Bytes, ApiError> {
    read_body(
        headers,
        body,
        MAX_FILE_BYTES,
        "the file is larger than 1 MiB: split it",
    )
    .await
}

/// The body of the request, at most `max` bytes; `too_large` is what a refusal says.
pub(crate) async fn read_body(
    headers: &HeaderMap,
    body: Body,
    max: usize,
    too_large: &'static str,
) -> Result<Bytes, ApiError> {
    let announced = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    if announced.is_some_and(|length| length > max) {
        return Err(ApiError::FileTooLarge(too_large));
    }
    axum::body::to_bytes(body, max)
        .await
        .map_err(|_| ApiError::FileTooLarge(too_large))
}

/// Why a file as a whole is refused. Each message says what to change.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FileError {
    NotUtf8,
    Empty,
    TooManyRows,
    Unreadable(String),
    MissingColumn(&'static str),
    UnknownColumns(Vec<String>),
    RepeatedColumn(String),
    /// A party file with none of the columns that say what the party is.
    MissingRoleColumn,
}

impl From<FileError> for ApiError {
    fn from(error: FileError) -> Self {
        Self::InvalidFile(error.to_string())
    }
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotUtf8 => f.write_str(
                "the file is not UTF-8: save it as CSV UTF-8 (a spreadsheet's plain \"CSV\" is often not)",
            ),
            Self::Empty => f.write_str("the file has no rows"),
            Self::TooManyRows => {
                write!(f, "the file has more than {MAX_IMPORT_ROWS} rows: split it")
            }
            Self::Unreadable(cause) => write!(f, "the file cannot be read as CSV: {cause}"),
            Self::MissingColumn(column) => write!(f, "the file has no `{column}` column"),
            Self::UnknownColumns(columns) => write!(
                f,
                "the file has columns this import does not know: {}",
                columns.join(", ")
            ),
            Self::RepeatedColumn(column) => write!(f, "the column `{column}` appears twice"),
            Self::MissingRoleColumn => f.write_str(
                "the file needs at least one of the columns customer, supplier, carrier, seller",
            ),
        }
    }
}

/// One row of the file: its cells by column name. A cell that is blank is not there.
pub(crate) struct Record<'a> {
    headers: &'a [String],
    cells: csv::StringRecord,
}

impl Record<'_> {
    pub(crate) fn get(&self, column: &str) -> Option<&str> {
        let index = self.headers.iter().position(|header| header == column)?;
        self.cells.get(index).filter(|cell| !cell.is_empty())
    }
}

/// The text with its columns found and checked, one row for each line after the header.
pub(crate) struct Table {
    headers: Vec<String>,
    /// With the number a spreadsheet shows for each: rows with nothing in them were left out.
    records: Vec<(usize, csv::StringRecord)>,
}

impl Table {
    pub(crate) fn has_column(&self, column: &str) -> bool {
        self.headers.iter().any(|header| header == column)
    }

    /// The rows, each turned into a `T` by `read`, which can refuse it with a message; the number
    /// is the one a spreadsheet shows (the header is row 1). Rows that are refused are returned
    /// apart, so that a file reports every bad row and not just the first.
    pub(crate) fn rows<T>(
        self,
        mut read: impl FnMut(&Record<'_>) -> Result<T, String>,
    ) -> (Vec<ImportRow<T>>, Vec<RowNote>) {
        let mut rows = Vec::new();
        let mut refused = Vec::new();
        for (row, cells) in self.records {
            // More cells than the header is almost always an unquoted separator inside a cell
            // (`Silva, Filhos`): reading it anyway would shift every cell after it.
            if cells.len() > self.headers.len() {
                refused.push(RowNote {
                    row,
                    message: format!(
                        "the row has {} cells and the header {}: a cell that holds a comma or a semicolon goes in quotes",
                        cells.len(),
                        self.headers.len()
                    ),
                });
                continue;
            }
            let record = Record {
                headers: &self.headers,
                cells,
            };
            match read(&record) {
                Ok(draft) => rows.push(ImportRow { row, draft }),
                Err(message) => refused.push(RowNote { row, message }),
            }
        }
        (rows, refused)
    }
}

/// Reads a CSV file whose header names its columns, in any order. A comma or a semicolon separates
/// them, whichever the header uses (Brazilian spreadsheets save with `;`). `columns` are the
/// names the import knows and `required` the ones it cannot do without; a column of another name
/// is refused rather than ignored, so a misspelled one is not silently dropped.
pub(crate) fn read_table(
    bytes: &[u8],
    columns: &[&str],
    required: &[&'static str],
) -> Result<Table, FileError> {
    let text = std::str::from_utf8(bytes).map_err(|_| FileError::NotUtf8)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first_line = text.lines().next().unwrap_or_default();
    let delimiter = if first_line.matches(';').count() > first_line.matches(',').count() {
        b';'
    } else {
        b','
    };

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .trim(csv::Trim::All)
        // A row with fewer cells than the header is a row whose last cells are blank; one with more
        // is refused for its row (see `Table::rows`), not the file.
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|error| FileError::Unreadable(error.to_string()))?
        .iter()
        .map(str::to_lowercase)
        .collect();
    if headers.iter().all(String::is_empty) {
        return Err(FileError::Empty);
    }
    // A spreadsheet often leaves a column with no name at the end (a separator too many): it has
    // nothing the import could read, so it is not one of the columns.
    let mut seen = HashSet::new();
    for header in headers.iter().filter(|header| !header.is_empty()) {
        if !seen.insert(header.as_str()) {
            return Err(FileError::RepeatedColumn(header.clone()));
        }
    }
    let unknown: Vec<String> = headers
        .iter()
        .filter(|header| !header.is_empty() && !columns.contains(&header.as_str()))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return Err(FileError::UnknownColumns(unknown));
    }
    if let Some(missing) = required
        .iter()
        .find(|column| !headers.iter().any(|header| header == *column))
    {
        return Err(FileError::MissingColumn(missing));
    }

    let mut records = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|error| FileError::Unreadable(error.to_string()))?;
        // Spreadsheets leave formatted rows with nothing in them at the end of a sheet.
        if record.iter().all(str::is_empty) {
            continue;
        }
        if records.len() == MAX_IMPORT_ROWS {
            return Err(FileError::TooManyRows);
        }
        records.push((index + 2, record));
    }
    if records.is_empty() {
        return Err(FileError::Empty);
    }
    Ok(Table { headers, records })
}

#[derive(Serialize)]
pub(crate) struct RowNoteResponse {
    row: usize,
    message: String,
}

impl From<RowNote> for RowNoteResponse {
    fn from(note: RowNote) -> Self {
        Self {
            row: note.row,
            message: note.message,
        }
    }
}

/// What an import did, or with `dry_run` would do. Nothing is created when a row is refused.
#[derive(Serialize)]
pub(crate) struct ImportResponse {
    dry_run: bool,
    valid: usize,
    created: usize,
    skipped: Vec<RowNoteResponse>,
    refused: Vec<RowNoteResponse>,
}

impl ImportResponse {
    /// `refused` holds the rows that could not be read and the ones the register refused, in the
    /// order of the file.
    pub(crate) fn new(dry_run: bool, report: ImportReport, mut refused: Vec<RowNote>) -> Self {
        refused.sort_by_key(|note| note.row);
        Self {
            dry_run,
            valid: report.valid,
            created: report.created,
            skipped: report.skipped.into_iter().map(Into::into).collect(),
            refused: refused.into_iter().map(Into::into).collect(),
        }
    }
}

/// A yes or a no as people write it in a spreadsheet; blank is no. `None` for anything else, so
/// that the row can say which cell it did not understand.
pub(crate) fn yes_or_no(cell: Option<&str>) -> Option<bool> {
    match cell.map(str::to_lowercase).as_deref() {
        None => Some(false),
        Some("sim" | "s" | "yes" | "y" | "true" | "1" | "x") => Some(true),
        Some("não" | "nao" | "n" | "no" | "false" | "0") => Some(false),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLUMNS: &[&str] = &["name", "document", "customer"];

    fn table(text: &str) -> Result<Table, FileError> {
        read_table(text.as_bytes(), COLUMNS, &["name"])
    }

    fn cells(text: &str) -> Vec<(usize, Option<String>, Option<String>)> {
        let (rows, refused) = table(text).unwrap().rows(|record| {
            Ok((
                record.get("name").map(str::to_owned),
                record.get("document").map(str::to_owned),
            ))
        });
        assert!(refused.is_empty());
        rows.into_iter()
            .map(|row| (row.row, row.draft.0, row.draft.1))
            .collect()
    }

    #[test]
    fn reads_columns_by_name_in_any_order_and_numbers_rows_like_a_spreadsheet() {
        let rows = cells("document,name\n529.982.247-25,Ana\n,Bruno\n");
        assert_eq!(
            rows,
            [
                (2, Some("Ana".into()), Some("529.982.247-25".into())),
                (3, Some("Bruno".into()), None),
            ]
        );
    }

    #[test]
    fn a_semicolon_a_byte_order_mark_and_quotes_are_understood() {
        let rows = cells("\u{feff}Name;Document\n\"Silva; Filhos\";\"123\"\n  Ana  ;  \n");
        assert_eq!(
            rows,
            [
                (2, Some("Silva; Filhos".into()), Some("123".into())),
                (3, Some("Ana".into()), None),
            ]
        );
    }

    #[test]
    fn a_quoted_line_break_is_one_row() {
        let rows = cells("name\n\"Padaria\nCentral\"\nBruno\n");
        assert_eq!(rows[0].1.as_deref(), Some("Padaria\nCentral"));
        assert_eq!(rows[1].0, 3);
    }

    #[test]
    fn a_file_that_cannot_be_read_says_why() {
        // A NUL is a cell like any other here: the register refuses it, for its row.
        assert!(table("name\n\u{0}\n").is_ok());
        let cases = [
            ("", FileError::Empty),
            ("name\n", FileError::Empty),
            (
                "name,name\nAna,Ana\n",
                FileError::RepeatedColumn("name".into()),
            ),
            ("document\n123\n", FileError::MissingColumn("name")),
            (
                "name,phone,cor\nAna,1,2\n",
                FileError::UnknownColumns(vec!["phone".into(), "cor".into()]),
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(table(text).err(), Some(expected), "{text:?}");
        }
        assert_eq!(
            read_table(&[0xff, 0xfe, b'a'], COLUMNS, &[]).err(),
            Some(FileError::NotUtf8)
        );
    }

    #[test]
    fn a_column_without_a_name_is_left_alone() {
        let rows = cells("name;document;\nAna;123;\n");
        assert_eq!(rows, [(2, Some("Ana".into()), Some("123".into()))]);
    }

    #[test]
    fn rows_with_nothing_in_them_are_left_out_and_the_others_keep_their_numbers() {
        let rows = cells("name;document\nAna;1\n;\nBruno;2\n;;\n");
        assert_eq!(
            rows.iter().map(|row| row.0).collect::<Vec<_>>(),
            [2, 4],
            "{rows:?}"
        );
        assert_eq!(table("name;document\n;\n;\n").err(), Some(FileError::Empty));
    }

    #[test]
    fn a_row_with_more_cells_than_the_header_is_refused_for_its_row_alone() {
        let (rows, refused) = table("name,document\nAna,1\nSilva, Filhos,2\nCarla\n")
            .unwrap()
            .rows(|record| Ok(record.get("name").map(str::to_owned)));
        // A row with fewer cells is fine: the rest are blank.
        assert_eq!(rows.iter().map(|r| r.row).collect::<Vec<_>>(), [2, 4]);
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].row, 3);
        assert!(refused[0].message.contains("3 cells and the header 2"));
    }

    #[test]
    fn at_most_the_limit_of_rows() {
        let file = |count: usize| format!("name\n{}", "Ana\n".repeat(count));
        assert!(table(&file(MAX_IMPORT_ROWS)).is_ok());
        assert_eq!(
            table(&file(MAX_IMPORT_ROWS + 1)).err(),
            Some(FileError::TooManyRows)
        );
    }

    #[test]
    fn a_row_that_is_refused_is_reported_apart_with_its_number() {
        let (rows, refused) =
            table("name\nAna\nBruno\nCarla\n")
                .unwrap()
                .rows(|record| match record.get("name") {
                    Some("Bruno") => Err("no Brunos".to_owned()),
                    other => Ok(other.map(str::to_owned)),
                });
        assert_eq!(rows.iter().map(|r| r.row).collect::<Vec<_>>(), [2, 4]);
        assert_eq!(
            refused,
            [RowNote {
                row: 3,
                message: "no Brunos".into()
            }]
        );
    }

    #[test]
    fn yes_and_no_as_people_write_them() {
        for yes in ["sim", "Sim", "S", "true", "1", "x", "YES"] {
            assert_eq!(yes_or_no(Some(yes)), Some(true), "{yes}");
        }
        for no in ["não", "nao", "N", "false", "0"] {
            assert_eq!(yes_or_no(Some(no)), Some(false), "{no}");
        }
        assert_eq!(yes_or_no(None), Some(false));
        assert_eq!(yes_or_no(Some("talvez")), None);
    }
}
