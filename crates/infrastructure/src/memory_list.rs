use conta_verde_application::{NameCursor, NameSort};
use uuid::Uuid;

/// One page of rows already narrowed by a query: ordered by name ignoring case, then by id (both
/// reversed when descending), starting right after `after`. It orders the way the SQL adapters do
/// for names whose order does not depend on the database collation.
pub(crate) fn keyset_page<T>(
    mut rows: Vec<T>,
    sort: NameSort,
    after: Option<&NameCursor>,
    limit: usize,
    key_of: impl Fn(&T) -> (String, Uuid),
) -> Vec<T> {
    let descending = sort == NameSort::NameDesc;
    let after_key = after.map(|cursor| (cursor.name.to_lowercase(), cursor.id));
    if let Some(after_key) = &after_key {
        rows.retain(|row| {
            let key = key_of(row);
            if descending {
                key < *after_key
            } else {
                key > *after_key
            }
        });
    }
    rows.sort_by_cached_key(&key_of);
    if descending {
        rows.reverse();
    }
    rows.truncate(limit);
    rows
}

/// The key lists are ordered by.
pub(crate) fn name_key(name: &str, id: Uuid) -> (String, Uuid) {
    (name.to_lowercase(), id)
}
