use std::time::Duration;

use conta_verde_application::{CnpjLookupError, CnpjRegistry, RegistryEntry};
use reqwest::StatusCode;
use serde::Deserialize;

const DEFAULT_BASE_URL: &str = "https://brasilapi.com.br/api/cnpj/v1";
// Data entry waits on this call, so a slow service must not hold a request for long.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
// BrasilAPI answers 429 to any request without a User-Agent, even the first one, so a client that
// does not identify itself never gets a lookup through.
const USER_AGENT: &str = concat!(
    "conta-verde/",
    env!("CARGO_PKG_VERSION"),
    " (+",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);

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
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(USER_AGENT)
            .build()?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }
}

#[derive(Deserialize)]
struct CnpjResponse {
    razao_social: String,
    descricao_situacao_cadastral: String,
}

impl CnpjRegistry for BrasilApiCnpjRegistry {
    async fn lookup(&self, cnpj: &str) -> Result<RegistryEntry, CnpjLookupError> {
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
                Ok(RegistryEntry {
                    legal_name: body.razao_social,
                    situation: body.descricao_situacao_cadastral,
                })
            }
            StatusCode::NOT_FOUND => Err(CnpjLookupError::NotFound),
            // 400 means the service disagrees with our check digits, 429 is the rate limit and
            // 5xx is its own trouble: none of them proves the company does not exist.
            status => Err(unavailable(cnpj, format_args!("answered {status}"))),
        }
    }
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
    use serde_json::json;

    use super::*;

    // A local stand-in for BrasilAPI: the tests never reach the real service.
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
        match cnpj.as_str() {
            "11222333000181" => (
                StatusCode::OK,
                axum::Json(json!({
                    "razao_social": "PADARIA CENTRAL LTDA",
                    "descricao_situacao_cadastral": "ATIVA",
                    "qsa": []
                })),
            ),
            "12ABC34501DE35" => (
                StatusCode::NOT_FOUND,
                axum::Json(json!({"type": "not_found"})),
            ),
            "00000000000191" => (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({"type": "bad_request"})),
            ),
            "11111111000191" => (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({"type": "rate_limited"})),
            ),
            "22222222000191" => (StatusCode::OK, axum::Json(json!({"unexpected": true}))),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({}))),
        }
    }

    fn fake_service() -> Router {
        Router::new().route("/api/cnpj/v1/{cnpj}", get(answer))
    }

    #[tokio::test]
    async fn maps_a_found_company() {
        let addr = serve(fake_service()).await;
        let entry = registry_at(addr, Duration::from_secs(2))
            .lookup("11222333000181")
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
        assert!(registry.lookup("11222333000181").await.is_ok());
        assert!(USER_AGENT.starts_with("conta-verde/"), "{USER_AGENT}");
    }

    #[tokio::test]
    async fn only_a_404_means_not_found() {
        let addr = serve(fake_service()).await;
        let registry = registry_at(addr, Duration::from_secs(2));
        assert_eq!(
            registry.lookup("12ABC34501DE35").await,
            Err(CnpjLookupError::NotFound)
        );
        for unreliable in [
            "00000000000191", // 400
            "11111111000191", // 429
            "33333333000191", // 500
            "22222222000191", // 200 with a body we cannot read
        ] {
            assert_eq!(
                registry.lookup(unreliable).await,
                Err(CnpjLookupError::Unavailable),
                "{unreliable}"
            );
        }
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
            registry_at(addr, Duration::from_millis(150))
                .lookup("11222333000181")
                .await,
            Err(CnpjLookupError::Unavailable)
        );
        assert!(started.elapsed() < Duration::from_secs(2));

        // Nothing listens on port 1.
        let nowhere =
            BrasilApiCnpjRegistry::with_endpoint("http://127.0.0.1:1", Duration::from_secs(1))
                .unwrap();
        assert_eq!(
            nowhere.lookup("11222333000181").await,
            Err(CnpjLookupError::Unavailable)
        );
    }
}
