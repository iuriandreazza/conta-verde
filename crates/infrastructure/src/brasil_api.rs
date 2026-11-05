use std::time::Duration;

use conta_verde_application::{CnpjLookupError, CnpjRegistry, RegistryEntry};
use conta_verde_domain::Cnpj;
use reqwest::StatusCode;
use serde::Deserialize;

const DEFAULT_BASE_URL: &str = "https://brasilapi.com.br/api/cnpj/v1";
// Data entry waits on this call, so a slow service must not hold a request for long.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
// BrasilAPI answers 429 to any request without a User-Agent, even the first one, so a client that
// does not identify itself never gets a lookup through.
pub(crate) const USER_AGENT: &str = concat!(
    "conta-verde/",
    env!("CARGO_PKG_VERSION"),
    " (+",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);
// The text comes from a third party and is stored and listed with every party: keep it bounded.
const MAX_LEGAL_NAME_CHARS: usize = 200;
const MAX_SITUATION_CHARS: usize = 60;

/// Looks CNPJs up on BrasilAPI, a free service built on Receita Federal's open data. It is run by
/// volunteers, not by the government: it has no uptime guarantee and data can lag the source.
pub struct BrasilApiCnpjRegistry {
    client: reqwest::Client,
    base_url: String,
}

impl BrasilApiCnpjRegistry {
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

/// One client for every BrasilAPI adapter: a timeout, because data entry waits on the call, and
/// the `User-Agent` the service insists on.
pub(crate) fn client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build()
}

#[derive(Deserialize)]
struct CnpjResponse {
    razao_social: String,
    descricao_situacao_cadastral: String,
}

impl CnpjRegistry for BrasilApiCnpjRegistry {
    async fn lookup(&self, cnpj: Cnpj<'_>) -> Result<RegistryEntry, CnpjLookupError> {
        let cnpj = cnpj.as_str();
        let response = self
            .client
            .get(format!("{}/{cnpj}", self.base_url))
            .send()
            .await
            .map_err(|error| unavailable(cnpj, format_args!("request failed: {error}")))?;

        match response.status() {
            StatusCode::OK => {
                let body: CnpjResponse = response.json().await.map_err(|error| {
                    unavailable(cnpj, format_args!("unexpected response body: {error}"))
                })?;
                let legal_name = bounded(&body.razao_social, MAX_LEGAL_NAME_CHARS);
                let situation = bounded(&body.descricao_situacao_cadastral, MAX_SITUATION_CHARS);
                // A 200 without a name or a situation is not an answer we can show or rely on.
                if legal_name.is_empty() || situation.is_empty() {
                    return Err(unavailable(
                        cnpj,
                        format_args!("answered without a name or situation"),
                    ));
                }
                Ok(RegistryEntry {
                    legal_name,
                    situation,
                })
            }
            StatusCode::NOT_FOUND => Err(CnpjLookupError::NotFound),
            // 400 means the service disagrees with our check digits, 429 is the rate limit and
            // 5xx is its own trouble: none of them proves the company does not exist.
            status => Err(unavailable(cnpj, format_args!("answered {status}"))),
        }
    }
}

/// Control characters and the invisible direction marks and overrides could scramble how the text
/// reads next to other text (React escapes HTML, but not these), so they are dropped.
pub(crate) fn bounded(text: &str, max_chars: usize) -> String {
    let visible: String = text
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
        .collect();
    visible.trim().chars().take(max_chars).collect()
}

fn unavailable(cnpj: &str, cause: std::fmt::Arguments<'_>) -> CnpjLookupError {
    tracing::warn!(cnpj, %cause, "CNPJ registry lookup unavailable");
    CnpjLookupError::Unavailable
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::Router;
    use axum::extract::Path;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::get;
    use conta_verde_domain::Document;
    use serde_json::json;

    use super::*;

    // A local stand-in for BrasilAPI: the tests never reach the real service. Each CNPJ below has
    // valid check digits and triggers one behavior.
    const FOUND: &str = "11.222.333/0001-81";
    const MISSING: &str = "12.ABC.345/01DE-35";
    const BAD_REQUEST: &str = "00.000.000/0001-91";
    const RATE_LIMITED: &str = "12.345.678/0001-95";
    const UNREADABLE_BODY: &str = "12.345.678/000A-08";
    const SERVER_ERROR: &str = "AA345678/0003-86";
    const HUGE_TEXT: &str = "AA345678/000A-29";
    const EMPTY_NAME: &str = "11.222.333/000A-96";
    const SCRAMBLED_TEXT: &str = "12.345.678/000B-80";

    async fn serve(router: Router) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        addr
    }

    fn registry_at(addr: SocketAddr, timeout: Duration) -> BrasilApiCnpjRegistry {
        BrasilApiCnpjRegistry::with_endpoint(&format!("http://{addr}/api/cnpj/v1"), timeout)
            .unwrap()
    }

    async fn lookup(
        registry: &BrasilApiCnpjRegistry,
        formatted: &str,
    ) -> Result<RegistryEntry, CnpjLookupError> {
        let document = Document::parse(formatted).unwrap();
        registry.lookup(document.cnpj().unwrap()).await
    }

    async fn answer(
        Path(cnpj): Path<String>,
        headers: HeaderMap,
    ) -> (StatusCode, axum::Json<serde_json::Value>) {
        // The real service does this: see USER_AGENT.
        if !headers.contains_key("user-agent") {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({"type": "rate_limited"})),
            );
        }
        let stored = |formatted: &str| formatted.replace(['.', '/', '-'], "");
        let found = |name: String| {
            (
                StatusCode::OK,
                axum::Json(json!({
                    "razao_social": name,
                    "descricao_situacao_cadastral": "ATIVA",
                    "qsa": []
                })),
            )
        };
        match cnpj {
            c if c == stored(FOUND) => found("PADARIA CENTRAL LTDA".into()),
            c if c == stored(HUGE_TEXT) => found("X".repeat(5_000)),
            c if c == stored(EMPTY_NAME) => found("  \u{200B} ".into()),
            // A right-to-left override and a bell in the middle of the name.
            c if c == stored(SCRAMBLED_TEXT) => found("PADARIA\u{202E}\u{7} LTDA".into()),
            c if c == stored(MISSING) => (
                StatusCode::NOT_FOUND,
                axum::Json(json!({"type": "not_found"})),
            ),
            c if c == stored(BAD_REQUEST) => (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({"type": "bad_request"})),
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
        Router::new().route("/api/cnpj/v1/{cnpj}", get(answer))
    }

    #[tokio::test]
    async fn maps_a_found_company() {
        let addr = serve(fake_service()).await;
        let entry = lookup(&registry_at(addr, Duration::from_secs(2)), FOUND)
            .await
            .unwrap();
        assert_eq!(entry.legal_name, "PADARIA CENTRAL LTDA");
        assert_eq!(entry.situation, "ATIVA");
    }

    #[tokio::test]
    async fn identifies_itself_to_the_service() {
        // The fake answers 429 without a User-Agent, like the real service; getting a company
        // back proves the header is sent.
        let addr = serve(fake_service()).await;
        let registry = registry_at(addr, Duration::from_secs(2));
        assert!(lookup(&registry, FOUND).await.is_ok());
        assert!(USER_AGENT.starts_with("conta-verde/"), "{USER_AGENT}");
    }

    #[tokio::test]
    async fn only_a_404_means_not_found() {
        let addr = serve(fake_service()).await;
        let registry = registry_at(addr, Duration::from_secs(2));
        assert_eq!(
            lookup(&registry, MISSING).await,
            Err(CnpjLookupError::NotFound)
        );
        for unreliable in [
            BAD_REQUEST,
            RATE_LIMITED,
            SERVER_ERROR,
            UNREADABLE_BODY,
            EMPTY_NAME,
        ] {
            assert_eq!(
                lookup(&registry, unreliable).await,
                Err(CnpjLookupError::Unavailable),
                "{unreliable}"
            );
        }
    }

    #[tokio::test]
    async fn bounds_the_text_it_stores() {
        let addr = serve(fake_service()).await;
        let entry = lookup(&registry_at(addr, Duration::from_secs(2)), HUGE_TEXT)
            .await
            .unwrap();
        assert_eq!(entry.legal_name.chars().count(), MAX_LEGAL_NAME_CHARS);
    }

    #[tokio::test]
    async fn drops_control_and_direction_characters_from_the_text() {
        let addr = serve(fake_service()).await;
        let entry = lookup(&registry_at(addr, Duration::from_secs(2)), SCRAMBLED_TEXT)
            .await
            .unwrap();
        assert_eq!(entry.legal_name, "PADARIA LTDA");
    }

    #[tokio::test]
    async fn slow_and_unreachable_services_are_unavailable() {
        let slow = Router::new().route(
            "/api/cnpj/v1/{cnpj}",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                "late"
            }),
        );
        let addr = serve(slow).await;
        let started = std::time::Instant::now();
        assert_eq!(
            lookup(&registry_at(addr, Duration::from_millis(150)), FOUND).await,
            Err(CnpjLookupError::Unavailable)
        );
        assert!(started.elapsed() < Duration::from_secs(2));

        // Nothing listens on port 1.
        let nowhere =
            BrasilApiCnpjRegistry::with_endpoint("http://127.0.0.1:1", Duration::from_secs(1))
                .unwrap();
        assert_eq!(
            lookup(&nowhere, FOUND).await,
            Err(CnpjLookupError::Unavailable)
        );
    }
}
