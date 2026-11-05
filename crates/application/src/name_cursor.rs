use uuid::Uuid;

/// Position of the last row of a page of a list ordered by name ignoring case, then by id (both
/// reversed for a descending sort), so the name and id of the last item are enough to resume right
/// after it in either direction. Every register listed by name shares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameCursor {
    pub name: String,
    pub id: Uuid,
}
