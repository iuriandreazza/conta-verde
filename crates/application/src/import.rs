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
