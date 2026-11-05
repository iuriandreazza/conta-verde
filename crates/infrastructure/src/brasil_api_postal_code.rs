use std::time::Duration;

use conta_verde_application::{PostalCodeDirectory, PostalCodeEntry, PostalCodeLookupError};
use conta_verde_domain::PostalCode;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::brasil_api::{bounded, client};

const DEFAULT_BASE_URL: &str = "https://brasilapi.com.br/api/cep/v2";
// Data entry waits on this call, so a slow service must not hold a request for long.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
// The text comes from a third party and goes into a form the user then checks: keep it bounded,
// at the lengths an address accepts.
const MAX_STREET_CHARS: usize = 150;
const MAX_DISTRICT_CHARS: usize = 80;
const MAX_CITY_CHARS: usize = 80;
const STATE_CHARS: usize = 2;

/// Looks postal codes up on BrasilAPI, which asks the Correios and other public sources. It is run
/// by volunteers, not by the government: it has no uptime guarantee, and it is slow when its
/// sources are.
pub struct BrasilApiPostalCodeDirectory {
    client: reqwest::Client,
    base_url: String,
}

impl BrasilApiPostalCodeDirectory {
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_endpoint(DEFAULT_BASE_URL, DEFAULT_TIMEOUT)
    }

    pub fn with_endpoint(base_url: &str, timeout: Duration) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: client(timeout)?,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }
}

#[derive(Deserialize)]
struct CepResponse {
    street: Option<String>,
    neighborhood: Option<String>,
    city: String,
    state: String,
}

impl PostalCodeDirectory for BrasilApiPostalCodeDirectory {
    async fn lookup(&self, code: &PostalCode) -> Result<PostalCodeEntry, PostalCodeLookupError> {
        let code = code.as_str();
        let response = self
            .client
            .get(format!("{}/{code}", self.base_url))
            .send()
            .await
            .map_err(|error| unavailable(code, format_args!("request failed: {error}")))?;

        match response.status() {
            StatusCode::OK => {
                let body: CepResponse = response.json().await.map_err(|error| {
                    unavailable(code, format_args!("unexpected response body: {error}"))
                })?;
                let city = bounded(&body.city, MAX_CITY_CHARS);
                let state = bounded(&body.state, STATE_CHARS + 1).to_ascii_uppercase();
                // A 200 without a city or a two-letter state is not an answer to fill a form with.
                if city.is_empty() || state.chars().count() != STATE_CHARS {
                    return Err(unavailable(
                        code,
                        format_args!("answered without a city or a state"),
                    ));
                }
                let optional = |text: Option<String>, max: usize| {
                    text.map(|text| bounded(&text, max))
                        .filter(|text| !text.is_empty())
                };
                Ok(PostalCodeEntry {
                    street: optional(body.street, MAX_STREET_CHARS),
                    district: optional(body.neighborhood, MAX_DISTRICT_CHARS),
                    city,
                    state,
                })
            }
            // The service answers 404 both for a code it does not know and when every one of its own
            // sources failed, so this does not prove the code does not exist.
            StatusCode::NOT_FOUND => Err(PostalCodeLookupError::NotFound),
            // 429 is the rate limit and 5xx is its own trouble.
            status => Err(unavailable(code, format_args!("answered {status}"))),
        }
    }
}

fn unavailable(code: &str, cause: std::fmt::Arguments<'_>) -> PostalCodeLookupError {
    tracing::warn!(code, %cause, "postal code lookup unavailable");
    PostalCodeLookupError::Unavailable
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::Router;
    use axum::extract::Path;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::get;
    use serde_json::json;

    use super::*;

    // A local stand-in for BrasilAPI: the tests never reach the real service. Each code below
    // triggers one behavior.
    const FOUND: &str = "01310-100";
    const NO_STREET: &str = "50000-001";
    const MISSING: &str = "50000-002";
    const RATE_LIMITED: &str = "50000-003";
    const NO_CITY: &str = "50000-004";
    const BAD_STATE: &str = "50000-005";
    const UNREADABLE_BODY: &str = "50000-006";
    const SCRAMBLED_TEXT: &str = "50000-007";
    const HUGE_TEXT: &str = "50000-008";

    async fn serve(router: Router) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        addr
    }

    fn directory_at(addr: SocketAddr, timeout: Duration) -> BrasilApiPostalCodeDirectory {
        BrasilApiPostalCodeDirectory::with_endpoint(&format!("http://{addr}/api/cep/v2"), timeout)
            .unwrap()
    }

    async fn lookup(
        directory: &BrasilApiPostalCodeDirectory,
        code: &str,
    ) -> Result<PostalCodeEntry, PostalCodeLookupError> {
        directory.lookup(&PostalCode::parse(code).unwrap()).await
    }

    async fn answer(
        Path(code): Path<String>,
        headers: HeaderMap,
    ) -> (StatusCode, axum::Json<serde_json::Value>) {
        // The real service does this for any BrasilAPI endpoint: see `brasil_api::USER_AGENT`.
        if !headers.contains_key("user-agent") {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({"type": "rate_limited"})),
            );
        }
        let stored = |formatted: &str| formatted.replace('-', "");
        let found = |street: serde_json::Value, city: &str, state: &str| {
            (
                StatusCode::OK,
                axum::Json(json!({
                    "cep": code,
                    "state": state,
                    "city": city,
                    "neighborhood": "Bela Vista",
                    "street": street,
                    "service": "open-cep"
                })),
            )
        };
        match code.as_str() {
            c if c == stored(FOUND) => found(json!("Avenida Paulista"), "São Paulo", "SP"),
            // A city with one code for all of it has an empty street.
            c if c == stored(NO_STREET) => found(json!(""), "Recife", "pe"),
            c if c == stored(NO_CITY) => found(json!("Rua A"), " ", "PE"),
            c if c == stored(BAD_STATE) => found(json!("Rua A"), "Recife", "Pernambuco"),
            c if c == stored(SCRAMBLED_TEXT) => found(json!("Rua\u{202E}\u{7} A"), "Recife", "PE"),
            c if c == stored(HUGE_TEXT) => found(json!("X".repeat(5_000)), "Recife", "PE"),
            c if c == stored(MISSING) => (
                StatusCode::NOT_FOUND,
                axum::Json(json!({"type": "service_error"})),
            ),
            c if c == stored(RATE_LIMITED) => (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({"type": "rate_limited"})),
            ),
            c if c == stored(UNREADABLE_BODY) => {
                (StatusCode::OK, axum::Json(json!({"unexpected": true})))
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({}))),
        }
    }

    fn fake_service() -> Router {
        Router::new().route("/api/cep/v2/{code}", get(answer))
    }

    #[tokio::test]
    async fn maps_a_found_address_and_identifies_itself() {
        // The fake answers 429 without a User-Agent, like the real service, so an answer proves
        // the header is sent.
        let addr = serve(fake_service()).await;
        let entry = lookup(&directory_at(addr, Duration::from_secs(2)), FOUND)
            .await
            .unwrap();
        assert_eq!(
            entry,
            PostalCodeEntry {
                street: Some("Avenida Paulista".into()),
                district: Some("Bela Vista".into()),
                city: "São Paulo".into(),
                state: "SP".into(),
            }
        );
    }

    #[tokio::test]
    async fn an_empty_street_is_no_street_and_the_state_is_upper_case() {
        let addr = serve(fake_service()).await;
        let entry = lookup(&directory_at(addr, Duration::from_secs(2)), NO_STREET)
            .await
            .unwrap();
        assert_eq!(entry.street, None);
        assert_eq!(entry.state, "PE");
    }

    #[tokio::test]
    async fn only_a_404_means_not_found() {
        let addr = serve(fake_service()).await;
        let directory = directory_at(addr, Duration::from_secs(2));
        assert_eq!(
            lookup(&directory, MISSING).await,
            Err(PostalCodeLookupError::NotFound)
        );
        for unreliable in [
            RATE_LIMITED,
            UNREADABLE_BODY,
            NO_CITY,
            BAD_STATE,
            "99999-999",
        ] {
            assert_eq!(
                lookup(&directory, unreliable).await,
                Err(PostalCodeLookupError::Unavailable),
                "{unreliable}"
            );
        }
    }

    #[tokio::test]
    async fn bounds_the_text_and_drops_control_and_direction_characters() {
        let addr = serve(fake_service()).await;
        let directory = directory_at(addr, Duration::from_secs(2));
        let scrambled = lookup(&directory, SCRAMBLED_TEXT).await.unwrap();
        assert_eq!(scrambled.street.as_deref(), Some("Rua A"));
        let huge = lookup(&directory, HUGE_TEXT).await.unwrap();
        assert_eq!(huge.street.unwrap().chars().count(), MAX_STREET_CHARS);
    }

    #[tokio::test]
    async fn slow_and_unreachable_services_are_unavailable() {
        let slow = Router::new().route(
            "/api/cep/v2/{code}",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "late"
            }),
        );
        let addr = serve(slow).await;
        let started = std::time::Instant::now();
        assert_eq!(
            lookup(&directory_at(addr, Duration::from_millis(150)), FOUND).await,
            Err(PostalCodeLookupError::Unavailable)
        );
        assert!(started.elapsed() < Duration::from_secs(2));

        // Nothing listens on port 1.
        let nowhere = BrasilApiPostalCodeDirectory::with_endpoint(
            "http://127.0.0.1:1",
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(
            lookup(&nowhere, FOUND).await,
            Err(PostalCodeLookupError::Unavailable)
        );
    }
}
