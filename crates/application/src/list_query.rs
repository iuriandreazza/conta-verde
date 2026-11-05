//! What every list query shares (ADR 0018); a register adds its own filters on top.

/// Longest search text accepted; a person types a name or a number, not a paragraph.
pub const MAX_SEARCH_CHARS: usize = 100;

pub(crate) const DEFAULT_PAGE_SIZE: usize = 50;
pub(crate) const MAX_PAGE_SIZE: usize = 200;

/// Every sort is by a column that is never empty, so a cursor can always be built from the last row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NameSort {
    #[default]
    NameAsc,
    NameDesc,
}

/// The text a search looks for. Control characters are dropped: nobody types them into a search
/// box, and a NUL is a storage error in Postgres, not a search. Blank text means no search.
pub(crate) fn clean_search(text: &str) -> Option<String> {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}
