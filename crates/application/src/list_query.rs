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

/// The page size asked for, kept between 1 and the most a page may have.
pub(crate) fn clamp_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE)
}

/// Turns the `limit + 1` rows a repository was asked for into a page and the cursor of the next
/// one: the extra row only tells whether another page exists, without a second query.
pub(crate) fn split_page<T, C>(mut rows: Vec<T>, limit: usize) -> (Vec<T>, Option<C>)
where
    C: for<'a> From<&'a T>,
{
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let next = if has_more {
        rows.last().map(C::from)
    } else {
        None
    };
    (rows, next)
}
