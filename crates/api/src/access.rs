use axum::extract::{FromRequestParts, MatchedPath, RawPathParams};
use axum::http::Method;
use axum::http::request::Parts;
use conta_verde_application::AuthError;
use conta_verde_domain::{Access, Actor, Area, CompanyId};
use uuid::Uuid;

use crate::error::ApiError;
use crate::session::require_token;
use crate::{Ports, SharedState};

/// Proof that the caller's session belongs to a member of the `{company_id}` in the route, whose
/// role reaches what the route does, and who the caller is, for the audit trail.
///
/// Declare it as a handler argument before any body or query extractor: it runs first, so
/// unauthenticated callers are rejected before anything else is parsed, and a company route
/// cannot be written without choosing whether to check access.
///
/// What the role must reach is read from the route itself ([ADR 0026]): the area is the segment
/// after the company in the route's pattern, and a `GET` reads while any other method writes. A
/// handler therefore cannot forget the check, and a route under a segment that no area claims is
/// refused to everyone until it is given one.
pub(crate) struct CompanyAccess(pub CompanyId, pub Actor);

/// The area a route of a company belongs to: the segment that follows `{company_id}`.
fn area_of(pattern: &str) -> Option<Area> {
    let mut segments = pattern.split('/').filter(|segment| !segment.is_empty());
    let (Some("companies"), Some("{company_id}"), Some(area)) =
        (segments.next(), segments.next(), segments.next())
    else {
        return None;
    };
    match area {
        "parties" | "postal-codes" => Some(Area::Parties),
        "products" => Some(Area::Products),
        "categories" | "cost-centers" | "bank-accounts" | "payment-methods" | "payment-terms"
        | "entries" | "transfers" | "balances" | "cash-flow" | "recurrences"
        | "statement-lines" => Some(Area::Finance),
        "members" | "invitations" => Some(Area::Members),
        _ => None,
    }
}

/// A request that only looks reads; anything else changes something.
fn access_of(method: &Method) -> Access {
    if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS {
        Access::Read
    } else {
        Access::Write
    }
}

impl<X: Ports> FromRequestParts<SharedState<X>> for CompanyAccess {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState<X>,
    ) -> Result<Self, ApiError> {
        // Owned, because extracting the path below borrows `parts` mutably.
        let token = require_token(&parts.headers)?.to_owned();
        let needed = parts
            .extensions
            .get::<MatchedPath>()
            .and_then(|pattern| area_of(pattern.as_str()))
            .map(|area| (area, access_of(&parts.method)));
        // Read by name, so routes with more path parameters (a party id after the company id)
        // work too. A missing or malformed id is indistinguishable from a company the caller
        // cannot see.
        let forbidden = || ApiError::Application(AuthError::Forbidden.into());
        let params = RawPathParams::from_request_parts(parts, state)
            .await
            .map_err(|_| forbidden())?;
        let raw_id = params
            .iter()
            .find_map(|(name, value)| (name == "company_id").then_some(value))
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(forbidden)?;
        let company_id = CompanyId::from_uuid(raw_id);
        let member = state.auth.authorize(&token, company_id).await?;
        // A member, but of a route no area claims: nobody's, until someone gives it an area.
        let not_allowed = || ApiError::Application(AuthError::NotAllowed.into());
        let (area, access) = needed.ok_or_else(not_allowed)?;
        member.require(area, access).map_err(|_| not_allowed())?;
        // The actor is what the audit trail will name: the user as they are called right now.
        let actor = Actor::User {
            id: member.user.id(),
            name: member.user.name().to_owned(),
        };
        Ok(Self(company_id, actor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_family_has_an_area_and_an_unknown_one_has_none() {
        assert_eq!(
            area_of("/companies/{company_id}/parties"),
            Some(Area::Parties)
        );
        assert_eq!(
            area_of("/companies/{company_id}/parties/{party_id}/history"),
            Some(Area::Parties)
        );
        assert_eq!(
            area_of("/companies/{company_id}/postal-codes/{code}"),
            Some(Area::Parties)
        );
        assert_eq!(
            area_of("/companies/{company_id}/products/import"),
            Some(Area::Products)
        );
        for finance in [
            "categories",
            "cost-centers",
            "bank-accounts",
            "payment-methods",
            "payment-terms",
            "entries",
            "transfers",
            "balances",
            "cash-flow",
            "recurrences",
            "statement-lines",
        ] {
            assert_eq!(
                area_of(&format!("/companies/{{company_id}}/{finance}")),
                Some(Area::Finance),
                "{finance}"
            );
        }
        assert_eq!(
            area_of("/companies/{company_id}/members"),
            Some(Area::Members)
        );
        // Fail closed: a segment nobody claimed, or a pattern that is not a company's at all.
        assert_eq!(area_of("/companies/{company_id}/unclaimed"), None);
        assert_eq!(area_of("/companies/{company_id}"), None);
        assert_eq!(area_of("/auth/me"), None);
    }

    #[test]
    fn only_looking_reads() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert_eq!(access_of(&method), Access::Read);
        }
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert_eq!(access_of(&method), Access::Write);
        }
    }
}
