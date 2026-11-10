//! The public `v1` API ([ADR 0087]): a curated, frozen set of resources for programs, authenticated by a company's key and
//! answered in RFC 9457 problem documents. It has types of its own and calls the same use cases as the web app, so no rule,
//! check or audit entry is skipped through it.

mod rate_limit;
mod resources;

use std::sync::Arc;

use axum::Router;
use axum::extract::{MatchedPath, Request, State};
use axum::http::header::{self, HeaderName, HeaderValue};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use conta_verde_application::AuthError;
use conta_verde_domain::{Access, Actor, Area, CalendarDate, CompanyId};
use serde::Serialize;

pub(crate) use rate_limit::RateLimiter;
use rate_limit::{Allowance, Verdict};

use crate::{Ports, SharedState};

/// The document that describes this API, served as it is checked in ([ADR 0087]).
const OPENAPI: &str = include_str!("../../../../docs/api/v1/openapi.json");

/// How a request of `v1` is judged: how many a key may make, and what time it is.
#[derive(Debug, Clone)]
pub struct V1Config {
    /// Requests a key may make in a minute, on average ([`RateLimiter`]: per process).
    pub rate_limit_per_minute: u32,
    pub clock: V1Clock,
}

impl Default for V1Config {
    fn default() -> Self {
        Self {
            rate_limit_per_minute: 300,
            clock: V1Clock::system(),
        }
    }
}

/// The moment now in Unix milliseconds, which a test replaces. It judges a key's expiry and its rate limit.
#[derive(Clone)]
pub struct V1Clock(Arc<dyn Fn() -> i64 + Send + Sync>);

impl V1Clock {
    pub fn system() -> Self {
        Self(Arc::new(crate::clock::now_millis))
    }

    pub fn of(now: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self(Arc::new(now))
    }

    pub(crate) fn now(&self) -> i64 {
        (self.0)()
    }
}

impl std::fmt::Debug for V1Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("V1Clock")
    }
}

/// Who is speaking, once the key was checked: the company it belongs to and the key as the actor of every write.
#[derive(Clone)]
pub(crate) struct Principal {
    pub company_id: CompanyId,
    pub actor: Actor,
}

/// Every route of `v1` that needs a key, and the area it reaches (a `GET` looks, anything else changes). One table, so that
/// a test can walk every route and a new one has to be placed in an area; `members`, `accounting` and `fiscal` are never in
/// it.
pub(crate) const ROUTE_AREAS: &[(&str, Area)] = &[
    ("/v1/parties", Area::Parties),
    ("/v1/parties/{party_id}", Area::Parties),
    ("/v1/products", Area::Products),
    ("/v1/products/{product_id}", Area::Products),
    ("/v1/products/{product_id}/stock", Area::Purchases),
    ("/v1/categories", Area::Finance),
    ("/v1/payment-terms", Area::Finance),
    ("/v1/sales", Area::Sales),
    ("/v1/sales/{sale_id}", Area::Sales),
    ("/v1/sales/{sale_id}/complete", Area::Sales),
    ("/v1/sales/{sale_id}/cancel", Area::Sales),
    ("/v1/sales/{sale_id}/charges", Area::Finance),
];

pub(crate) fn area_of(pattern: &str) -> Option<Area> {
    ROUTE_AREAS
        .iter()
        .find_map(|(known, area)| (*known == pattern).then_some(*area))
}

pub(crate) fn routes<X: Ports>(state: SharedState<X>) -> Router<SharedState<X>> {
    Router::new()
        .route(
            "/v1/parties",
            get(resources::list_parties::<X>).post(resources::register_party::<X>),
        )
        .route("/v1/parties/{party_id}", get(resources::get_party::<X>))
        .route("/v1/products", get(resources::list_products::<X>))
        .route(
            "/v1/products/{product_id}",
            get(resources::get_product::<X>),
        )
        .route(
            "/v1/products/{product_id}/stock",
            get(resources::product_stock::<X>),
        )
        .route("/v1/categories", get(resources::list_categories::<X>))
        .route("/v1/payment-terms", get(resources::list_payment_terms::<X>))
        .route(
            "/v1/sales",
            get(resources::list_sales::<X>).post(resources::register_sale::<X>),
        )
        .route("/v1/sales/{sale_id}", get(resources::get_sale::<X>))
        .route(
            "/v1/sales/{sale_id}/complete",
            post(resources::complete_sale::<X>),
        )
        .route(
            "/v1/sales/{sale_id}/cancel",
            post(resources::cancel_sale::<X>),
        )
        .route(
            "/v1/sales/{sale_id}/charges",
            get(resources::sale_charges::<X>),
        )
        .route_layer(axum::middleware::from_fn_with_state(state, gate::<X>))
        // The document is public: whoever writes a client reads it before having a key.
        .route("/v1/openapi.json", get(openapi))
}

async fn openapi() -> Response {
    ([(header::CONTENT_TYPE, "application/json")], OPENAPI).into_response()
}

/// Checks the key, its scope and its rate before any handler runs, and puts what the answer needs on it: the `RateLimit-*`
/// headers on every answer, and a problem document in the place of any error.
async fn gate<X: Ports>(
    State(state): State<SharedState<X>>,
    mut request: Request,
    next: Next,
) -> Response {
    let response = match admit(&state, &mut request).await {
        Ok(allowance) => {
            let mut response = next.run(request).await;
            allowance.write_to(response.headers_mut());
            response
        }
        Err(refusal) => return refusal.into_response(),
    };
    problem_for_errors(response).await
}

/// What stops a request at the gate.
enum Refusal {
    Unauthenticated,
    Scope(&'static str),
    Rate(Verdict),
    Storage,
}

impl Refusal {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthenticated => {
                let mut response = problem(
                    StatusCode::UNAUTHORIZED,
                    "unauthenticated",
                    "send a valid API key as `Authorization: Bearer <key>`",
                );
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
                response
            }
            Self::Scope(detail) => problem(StatusCode::FORBIDDEN, "insufficient_scope", detail),
            Self::Rate(Verdict::Refused {
                limit,
                retry_after_seconds,
            }) => {
                let mut response = problem(
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate_limited",
                    "too many requests: wait before trying again",
                );
                let headers = response.headers_mut();
                headers.insert(header::RETRY_AFTER, number(retry_after_seconds));
                headers.insert(RATE_LIMIT, number(u64::from(limit)));
                headers.insert(RATE_LIMIT_REMAINING, number(0));
                headers.insert(RATE_LIMIT_RESET, number(retry_after_seconds));
                response
            }
            Self::Rate(Verdict::Allowed(_)) => unreachable!("an allowed request is not refused"),
            Self::Storage => problem(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "internal error",
            ),
        }
    }
}

async fn admit<X: Ports>(
    state: &SharedState<X>,
    request: &mut Request,
) -> Result<Allowance, Refusal> {
    let token = bearer(request.headers()).ok_or(Refusal::Unauthenticated)?;
    let now = state.config.v1.clock.now();
    let today =
        CalendarDate::from_unix_days(now.div_euclid(86_400_000)).unwrap_or(CalendarDate::EARLIEST);
    let key = match state.api_keys.authenticate(token, now, today).await {
        Ok(key) => key,
        Err(conta_verde_application::ApplicationError::Auth(AuthError::Unauthenticated)) => {
            return Err(Refusal::Unauthenticated);
        }
        Err(error) => {
            tracing::error!(%error, "authenticating an API key failed");
            return Err(Refusal::Storage);
        }
    };
    let allowance = match state.limiter.check(key.id(), now) {
        Verdict::Allowed(allowance) => allowance,
        refused => return Err(Refusal::Rate(refused)),
    };
    let area = request
        .extensions()
        .get::<MatchedPath>()
        .and_then(|pattern| area_of(pattern.as_str()));
    let access = if matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) {
        Access::Read
    } else {
        Access::Write
    };
    // A route nobody placed in an area is refused to every key until someone does.
    match area {
        Some(area) if key.allows(area, access) => {}
        Some(_) => {
            return Err(Refusal::Scope(match access {
                Access::Read => "the key has no scope that reads this resource",
                Access::Write => "the key has no scope that changes this resource",
            }));
        }
        None => return Err(Refusal::Scope("no key may use this route")),
    }
    request.extensions_mut().insert(Principal {
        company_id: key.company_id(),
        actor: Actor::ApiKey {
            id: key.id(),
            name: key.name().to_owned(),
        },
    });
    Ok(allowance)
}

/// The key of `Authorization: Bearer <key>`; a cookie is never looked at.
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty()).then(|| token.trim())
}

const RATE_LIMIT: HeaderName = HeaderName::from_static("ratelimit-limit");
const RATE_LIMIT_REMAINING: HeaderName = HeaderName::from_static("ratelimit-remaining");
const RATE_LIMIT_RESET: HeaderName = HeaderName::from_static("ratelimit-reset");

fn number(value: u64) -> HeaderValue {
    HeaderValue::from(value)
}

impl Allowance {
    fn write_to(&self, headers: &mut HeaderMap) {
        headers.insert(RATE_LIMIT, number(u64::from(self.limit)));
        headers.insert(RATE_LIMIT_REMAINING, number(u64::from(self.remaining)));
        headers.insert(RATE_LIMIT_RESET, number(self.reset_seconds));
    }
}

/// An RFC 9457 problem document: `type` is a URN built from the stable `code`, which a client may switch on.
#[derive(Serialize)]
struct Problem<'a> {
    r#type: String,
    title: &'static str,
    status: u16,
    detail: &'a str,
    code: &'static str,
}

fn problem(status: StatusCode, code: &'static str, detail: &str) -> Response {
    let body = Problem {
        r#type: format!("urn:conta-verde:problem:{code}"),
        title: status.canonical_reason().unwrap_or("Error"),
        status: status.as_u16(),
        detail,
        code,
    };
    let mut response = (
        status,
        serde_json::to_vec(&body).expect("a problem document is always JSON"),
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    response
}

/// The code a status is told by. Finer codes may be added later without breaking a client that switches on these.
fn code_of(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST
        | StatusCode::UNPROCESSABLE_ENTITY
        | StatusCode::PAYLOAD_TOO_LARGE
        | StatusCode::UNSUPPORTED_MEDIA_TYPE => "invalid_request",
        StatusCode::UNAUTHORIZED => "unauthenticated",
        StatusCode::FORBIDDEN => "insufficient_scope",
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED => "not_found",
        StatusCode::CONFLICT | StatusCode::GONE => "conflict",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        _ => "internal",
    }
}

/// Turns the `{"error": "..."}` of the use cases and of the extractors into a problem document, so that `v1` has one error
/// shape whatever stopped the request. Anything that is not an error passes untouched.
async fn problem_for_errors(response: Response) -> Response {
    let status = response.status();
    if !(status.is_client_error() || status.is_server_error())
        || response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|value| value == "application/problem+json")
    {
        return response;
    }
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 64 * 1024)
        .await
        .unwrap_or_default();
    let detail = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_owned))
        .or_else(|| {
            String::from_utf8(bytes.to_vec())
                .ok()
                .filter(|text| !text.is_empty())
        })
        .unwrap_or_else(|| status.canonical_reason().unwrap_or("error").to_owned());
    let mut replaced = problem(status, code_of(status), &detail);
    // Whatever the answer already said about rate and retry stays.
    for (name, value) in &parts.headers {
        if name != header::CONTENT_TYPE && name != header::CONTENT_LENGTH {
            replaced.headers_mut().insert(name.clone(), value.clone());
        }
    }
    replaced
}
