mod access;
mod auth;
mod error;
mod parties;
mod session;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::{Json, Router};
use conta_verde_application::{
    AuthService, IdentityRepository, PartyRepository, PartyService, SecretService,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default)]
pub struct ApiConfig {
    /// Adds the `Secure` flag to the session cookie. Turn it on whenever the API is served over HTTPS.
    pub cookie_secure: bool,
}

pub(crate) struct AppState<R, I, S> {
    parties: PartyService<R>,
    auth: AuthService<I, S>,
    config: ApiConfig,
}

pub fn router<R, I, S>(
    parties: PartyService<R>,
    auth: AuthService<I, S>,
    config: ApiConfig,
) -> Router
where
    R: PartyRepository + 'static,
    I: IdentityRepository + 'static,
    S: SecretService + 'static,
{
    let state = Arc::new(AppState {
        parties,
        auth,
        config,
    });
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(auth::register::<R, I, S>))
        .route("/auth/login", post(auth::login::<R, I, S>))
        .route("/auth/logout", post(auth::logout::<R, I, S>))
        .route("/auth/me", get(auth::me::<R, I, S>))
        .route(
            "/companies/{company_id}/parties",
            get(parties::list_parties::<R, I, S>).post(parties::register_party::<R, I, S>),
        )
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use conta_verde_infrastructure::{
        Argon2SecretService, InMemoryIdentityRepository, InMemoryPartyRepository,
    };
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;

    fn app() -> Router {
        router(
            PartyService::new(InMemoryPartyRepository::default()),
            AuthService::new(
                InMemoryIdentityRepository::default(),
                Argon2SecretService::new().unwrap(),
                Duration::from_secs(3600),
            ),
            ApiConfig::default(),
        )
    }

    struct Reply {
        status: StatusCode,
        set_cookie: Option<String>,
        body: Value,
    }

    async fn send(app: &Router, request: Request<Body>) -> Reply {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .map(|value| value.to_str().unwrap().to_owned());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        Reply {
            status,
            set_cookie,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    fn json_request(method: &str, uri: &str, cookie: Option<&str>, body: Value) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }

    fn get_request(uri: &str, cookie: Option<&str>) -> Request<Body> {
        let mut builder = Request::get(uri);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        builder.body(Body::empty()).unwrap()
    }

    /// Registers a user and returns the cookie header to send back plus their company id.
    async fn sign_up(app: &Router, email: &str) -> (String, Uuid) {
        let reply = send(
            app,
            json_request(
                "POST",
                "/auth/register",
                None,
                json!({
                    "name": "Ana",
                    "email": email,
                    "password": "correct horse",
                    "company_name": "Padaria"
                }),
            ),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED);
        let cookie = reply.set_cookie.unwrap();
        let pair = cookie.split(';').next().unwrap().to_owned();
        let company_id = reply.body["companies"][0]["id"].as_str().unwrap();
        (pair, company_id.parse().unwrap())
    }

    fn party_body() -> Value {
        json!({
            "name": "Padaria Central",
            "document": "11.222.333/0001-81",
            "is_customer": true,
            "is_supplier": false
        })
    }

    fn parties_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/parties")
    }

    #[tokio::test]
    async fn health_reports_ok() {
        let reply = send(&app(), get_request("/health", None)).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.body["status"], "ok");
    }

    #[tokio::test]
    async fn registration_sets_a_hardened_session_cookie() {
        let app = app();
        let reply = send(
            &app,
            json_request(
                "POST",
                "/auth/register",
                None,
                json!({
                    "name": "Ana",
                    "email": "ana@example.com",
                    "password": "correct horse",
                    "company_name": "Padaria"
                }),
            ),
        )
        .await;
        let cookie = reply.set_cookie.unwrap();
        assert!(cookie.starts_with("cv_session="));
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
        assert!(!reply.body.to_string().contains("password"));
    }

    #[tokio::test]
    async fn me_requires_a_session_and_reports_companies() {
        let app = app();
        let anonymous = send(&app, get_request("/auth/me", None)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let reply = send(&app, get_request("/auth/me", Some(&cookie))).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.body["user"]["email"], "ana@example.com");
        assert_eq!(
            reply.body["companies"][0]["id"],
            company_id.to_string().as_str()
        );
    }

    #[tokio::test]
    async fn login_checks_credentials_and_logout_ends_the_session() {
        let app = app();
        sign_up(&app, "ana@example.com").await;

        let wrong = send(
            &app,
            json_request(
                "POST",
                "/auth/login",
                None,
                json!({ "email": "ana@example.com", "password": "nope nope" }),
            ),
        )
        .await;
        assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
        assert!(wrong.set_cookie.is_none());

        let ok = send(
            &app,
            json_request(
                "POST",
                "/auth/login",
                None,
                json!({ "email": "ana@example.com", "password": "correct horse" }),
            ),
        )
        .await;
        assert_eq!(ok.status, StatusCode::OK);
        let cookie = ok.set_cookie.unwrap().split(';').next().unwrap().to_owned();

        let logout = send(
            &app,
            json_request("POST", "/auth/logout", Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(logout.status, StatusCode::NO_CONTENT);
        assert!(logout.set_cookie.unwrap().contains("Max-Age=0"));

        let after = send(&app, get_request("/auth/me", Some(&cookie))).await;
        assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn duplicate_email_conflicts_and_weak_password_is_unprocessable() {
        let app = app();
        sign_up(&app, "ana@example.com").await;

        let register = |email: &str, password: &str| {
            json_request(
                "POST",
                "/auth/register",
                None,
                json!({
                    "name": "Outra",
                    "email": email,
                    "password": password,
                    "company_name": "Outra"
                }),
            )
        };
        let duplicate = send(&app, register("ANA@example.com", "correct horse")).await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);
        let weak = send(&app, register("new@example.com", "short")).await;
        assert_eq!(weak.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn parties_require_a_session() {
        let app = app();
        let (_, company_id) = sign_up(&app, "ana@example.com").await;
        let reply = send(&app, get_request(&parties_uri(company_id), None)).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn anonymous_callers_are_rejected_before_the_body_is_parsed() {
        let app = app();
        let (_, company_id) = sign_up(&app, "ana@example.com").await;
        let malformed = json!({ "unexpected": true });
        let reply = send(
            &app,
            json_request("POST", &parties_uri(company_id), None, malformed),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn registers_then_lists_a_party() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);

        let created = send(
            &app,
            json_request("POST", &uri, Some(&cookie), party_body()),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["document"], "11222333000181");

        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(listed.status, StatusCode::OK);
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        assert!(listed.body["next_cursor"].is_null());
    }

    #[tokio::test]
    async fn lists_parties_page_by_page() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);
        for name in ["Carla", "Ana", "Bruno"] {
            let body = json!({ "name": name, "is_customer": true, "is_supplier": false });
            send(&app, json_request("POST", &uri, Some(&cookie), body)).await;
        }

        let first = send(&app, get_request(&format!("{uri}?limit=2"), Some(&cookie))).await;
        let names = |reply: &Reply| -> Vec<String> {
            reply.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["name"].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(names(&first), ["Ana", "Bruno"]);

        // Cursors contain `|` and spaces in general, so clients must percent-encode them.
        let cursor = first.body["next_cursor"]
            .as_str()
            .unwrap()
            .replace('|', "%7C");
        let second = send(
            &app,
            get_request(&format!("{uri}?limit=2&after={cursor}"), Some(&cookie)),
        )
        .await;
        assert_eq!(names(&second), ["Carla"]);
        assert!(second.body["next_cursor"].is_null());

        let bad = send(
            &app,
            get_request(&format!("{uri}?after=garbage"), Some(&cookie)),
        )
        .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn other_users_and_unknown_companies_look_the_same() {
        let app = app();
        let (_, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, _) = sign_up(&app, "bia@example.com").await;

        let foreign = send(
            &app,
            get_request(&parties_uri(company_id), Some(&stranger_cookie)),
        )
        .await;
        let unknown = send(
            &app,
            get_request(&parties_uri(Uuid::new_v4()), Some(&stranger_cookie)),
        )
        .await;
        assert_eq!(foreign.status, StatusCode::NOT_FOUND);
        assert_eq!(foreign.status, unknown.status);
        assert_eq!(foreign.body, unknown.body);

        let write = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&stranger_cookie),
                party_body(),
            ),
        )
        .await;
        assert_eq!(write.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_party_is_unprocessable_and_duplicate_document_conflicts() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);

        let invalid = json!({ "name": "", "is_customer": true, "is_supplier": false });
        let reply = send(&app, json_request("POST", &uri, Some(&cookie), invalid)).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(reply.body["error"], "name must not be empty");

        send(
            &app,
            json_request("POST", &uri, Some(&cookie), party_body()),
        )
        .await;
        let duplicate = send(
            &app,
            json_request("POST", &uri, Some(&cookie), party_body()),
        )
        .await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);
    }
}
