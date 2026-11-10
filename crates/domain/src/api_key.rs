use crate::name::clean_name;
use crate::{Access, Actor, ApiKeyId, Area, CalendarDate, CompanyId, DomainError};

/// How many keys a company may keep, revoked ones included: they stay in the list so that the history of what a key
/// did can be reached ([ADR 0087]).
pub const MAX_API_KEYS: usize = 50;

/// The longest a rotation lets the old key live, in hours.
pub const MAX_ROTATION_GRACE_HOURS: u32 = 168;

/// The areas a key may be given. **Never** `members` (who belongs to the company), `accounting` (the closing of the books)
/// or `fiscal` (documents with legal weight): an integration that needs them is not what a key is for.
pub const API_KEY_AREAS: [Area; 5] = [
    Area::Parties,
    Area::Products,
    Area::Finance,
    Area::Sales,
    Area::Purchases,
];

/// One thing a key may do: look at an area, or change it (which includes looking), written `area:read` or `area:write`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiScope {
    area: Area,
    access: Access,
}

impl ApiScope {
    pub fn new(area: Area, access: Access) -> Result<Self, DomainError> {
        if API_KEY_AREAS.contains(&area) {
            Ok(Self { area, access })
        } else {
            Err(DomainError::InvalidApiScope)
        }
    }

    /// Reads `sales:write`. Anything else, including an area a key may not have, is refused.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let (area, access) = raw.split_once(':').ok_or(DomainError::InvalidApiScope)?;
        let area = Area::ALL
            .into_iter()
            .find(|known| known.as_str() == area)
            .ok_or(DomainError::InvalidApiScope)?;
        let access = match access {
            "read" => Access::Read,
            "write" => Access::Write,
            _ => return Err(DomainError::InvalidApiScope),
        };
        Self::new(area, access)
    }

    pub fn area(&self) -> Area {
        self.area
    }

    pub fn access(&self) -> Access {
        self.access
    }

    /// The text the API sends and the database stores; it does not change.
    pub fn key(&self) -> String {
        let access = match self.access {
            Access::Read => "read",
            Access::Write => "write",
        };
        format!("{}:{access}", self.area.as_str())
    }
}

/// Unvalidated input for making a key.
#[derive(Debug, Clone)]
pub struct ApiKeyDraft {
    pub name: String,
    /// `area:read` or `area:write`, at least one.
    pub scopes: Vec<String>,
    /// How long the key lives from the moment it is made, in hours; `None` for a key that does not expire.
    pub expires_in_hours: Option<u32>,
}

/// A key a program uses to speak to one company's `v1` API ([ADR 0087]). Its secret is shown once when it is made and only
/// the digest is kept, outside this type: what the key **is** is its name, its scopes and its life.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKey {
    id: ApiKeyId,
    company_id: CompanyId,
    name: String,
    /// The first characters of the secret, kept so that a person can tell keys apart; never enough to use one.
    label: String,
    /// Sorted, once each, and a `write` takes the `read` of its area with it.
    scopes: Vec<ApiScope>,
    /// Unix milliseconds. The key is refused from this moment on.
    expires_at: Option<i64>,
    revoked: bool,
    /// Who made it and when (Unix milliseconds); the name is a snapshot.
    created_by: Actor,
    created_at: i64,
    /// The key this one replaced when it was rotated.
    rotated_from: Option<ApiKeyId>,
    /// The last day it was used (bookkeeping, not audited).
    last_used_on: Option<CalendarDate>,
}

impl ApiKey {
    pub fn new(
        id: ApiKeyId,
        company_id: CompanyId,
        label: String,
        draft: ApiKeyDraft,
        created_by: Actor,
        now_millis: i64,
        rotated_from: Option<ApiKeyId>,
    ) -> Result<Self, DomainError> {
        let scopes = clean_scopes(&draft.scopes)?;
        let expires_at = match draft.expires_in_hours {
            None => None,
            Some(hours) if (1..=24 * 365 * 5).contains(&hours) => {
                Some(now_millis.saturating_add(i64::from(hours) * 3_600_000))
            }
            Some(_) => return Err(DomainError::InvalidApiKeyExpiry),
        };
        Ok(Self {
            id,
            company_id,
            name: clean_name(&draft.name)?,
            label,
            scopes,
            expires_at,
            revoked: false,
            created_by,
            created_at: now_millis,
            rotated_from,
            last_used_on: None,
        })
    }

    /// The key as it was stored.
    #[allow(clippy::too_many_arguments)]
    pub fn restored(
        id: ApiKeyId,
        company_id: CompanyId,
        name: String,
        label: String,
        scopes: Vec<ApiScope>,
        expires_at: Option<i64>,
        revoked: bool,
        created_by: Actor,
        created_at: i64,
        rotated_from: Option<ApiKeyId>,
        last_used_on: Option<CalendarDate>,
    ) -> Self {
        Self {
            id,
            company_id,
            name,
            label,
            scopes,
            expires_at,
            revoked,
            created_by,
            created_at,
            rotated_from,
            last_used_on,
        }
    }

    /// The same key, to be refused from `instant` on, unless it was to be refused sooner already.
    pub fn expiring_by(&self, instant: i64) -> Self {
        let mut next = self.clone();
        next.expires_at = Some(self.expires_at.map_or(instant, |own| own.min(instant)));
        next
    }

    pub fn revoked_now(&self) -> Self {
        let mut next = self.clone();
        next.revoked = true;
        next
    }

    pub fn used_on(&self, day: CalendarDate) -> Self {
        let mut next = self.clone();
        next.last_used_on = Some(day);
        next
    }

    pub fn id(&self) -> ApiKeyId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn scopes(&self) -> &[ApiScope] {
        &self.scopes
    }

    pub fn expires_at(&self) -> Option<i64> {
        self.expires_at
    }

    pub fn is_revoked(&self) -> bool {
        self.revoked
    }

    pub fn created_by(&self) -> &Actor {
        &self.created_by
    }

    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    pub fn rotated_from(&self) -> Option<ApiKeyId> {
        self.rotated_from
    }

    pub fn last_used_on(&self) -> Option<CalendarDate> {
        self.last_used_on
    }

    /// Whether the key may still be used at `now` (Unix milliseconds).
    pub fn is_usable_at(&self, now: i64) -> bool {
        !self.revoked && self.expires_at.is_none_or(|at| now < at)
    }

    /// Whether a scope of the key reaches the area as far as `access`.
    pub fn allows(&self, area: Area, access: Access) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.area == area && scope.access >= access)
    }

    /// The scopes as the API and the trail write them, one text per scope.
    pub fn scope_keys(&self) -> Vec<String> {
        self.scopes.iter().map(ApiScope::key).collect()
    }
}

/// At least one scope, each valid, once each, sorted by name, and without a `read` an area's `write` already includes.
fn clean_scopes(raw: &[String]) -> Result<Vec<ApiScope>, DomainError> {
    let mut scopes = raw
        .iter()
        .map(|text| ApiScope::parse(text))
        .collect::<Result<Vec<_>, _>>()?;
    if scopes.is_empty() {
        return Err(DomainError::NoApiScopes);
    }
    scopes.sort_by_key(ApiScope::key);
    scopes.dedup();
    let writes: Vec<Area> = scopes
        .iter()
        .filter(|scope| scope.access == Access::Write)
        .map(|scope| scope.area)
        .collect();
    scopes.retain(|scope| scope.access == Access::Write || !writes.contains(&scope.area));
    Ok(scopes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UserId;

    fn actor() -> Actor {
        Actor::User {
            id: UserId::generate(),
            name: "Ana".into(),
        }
    }

    fn make(scopes: &[&str], hours: Option<u32>) -> Result<ApiKey, DomainError> {
        ApiKey::new(
            ApiKeyId::generate(),
            CompanyId::generate(),
            "cvk_AbCd".into(),
            ApiKeyDraft {
                name: " Loja online ".into(),
                scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
                expires_in_hours: hours,
            },
            actor(),
            1_000,
            None,
        )
    }

    #[test]
    fn a_scope_is_an_area_and_an_access_and_never_a_forbidden_area() {
        assert_eq!(ApiScope::parse("sales:write").unwrap().key(), "sales:write");
        for refused in [
            "sales",
            "sales:admin",
            "Sales:read",
            "nowhere:read",
            "members:read",
            "accounting:write",
            "fiscal:read",
            "",
        ] {
            assert_eq!(
                ApiScope::parse(refused),
                Err(DomainError::InvalidApiScope),
                "{refused}"
            );
        }
    }

    #[test]
    fn scopes_are_sorted_once_each_and_a_write_takes_its_read_with_it() {
        let key = make(
            &["sales:read", "parties:read", "sales:write", "parties:read"],
            None,
        )
        .unwrap();
        assert_eq!(key.scope_keys(), ["parties:read", "sales:write"]);
        assert_eq!(make(&[], None), Err(DomainError::NoApiScopes));
    }

    #[test]
    fn a_key_reaches_what_its_scopes_name_and_nothing_else() {
        let key = make(&["sales:write", "products:read"], None).unwrap();
        assert!(key.allows(Area::Sales, Access::Write));
        assert!(key.allows(Area::Sales, Access::Read));
        assert!(key.allows(Area::Products, Access::Read));
        assert!(!key.allows(Area::Products, Access::Write));
        assert!(!key.allows(Area::Parties, Access::Read));
        assert!(!key.allows(Area::Members, Access::Read));
    }

    #[test]
    fn a_key_expires_at_an_instant_and_is_refused_once_revoked() {
        let key = make(&["parties:read"], Some(2)).unwrap();
        assert_eq!(key.expires_at(), Some(1_000 + 2 * 3_600_000));
        assert!(key.is_usable_at(1_000 + 2 * 3_600_000 - 1));
        assert!(!key.is_usable_at(1_000 + 2 * 3_600_000));
        assert!(!key.revoked_now().is_usable_at(1_000));
        assert!(
            make(&["parties:read"], None)
                .unwrap()
                .is_usable_at(i64::MAX)
        );
        assert_eq!(
            make(&["parties:read"], Some(0)),
            Err(DomainError::InvalidApiKeyExpiry)
        );
    }

    #[test]
    fn a_rotation_never_lengthens_the_life_of_the_old_key() {
        let key = make(&["parties:read"], Some(1)).unwrap();
        assert_eq!(key.expiring_by(i64::MAX).expires_at(), key.expires_at());
        assert_eq!(key.expiring_by(5_000).expires_at(), Some(5_000));
        let forever = make(&["parties:read"], None).unwrap();
        assert_eq!(forever.expiring_by(7_000).expires_at(), Some(7_000));
    }

    #[test]
    fn the_name_is_trimmed_and_cannot_be_blank() {
        assert_eq!(make(&["parties:read"], None).unwrap().name(), "Loja online");
        let blank = ApiKey::new(
            ApiKeyId::generate(),
            CompanyId::generate(),
            "cvk_AbCd".into(),
            ApiKeyDraft {
                name: "  ".into(),
                scopes: vec!["parties:read".into()],
                expires_in_hours: None,
            },
            actor(),
            0,
            None,
        );
        assert_eq!(blank, Err(DomainError::EmptyName));
    }
}
