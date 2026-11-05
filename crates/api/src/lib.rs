mod access;
mod auth;
mod categories;
mod cost_centers;
mod cursor;
mod error;
mod history;
mod json;
mod list_params;
mod parties;
mod products;
mod session;

use std::sync::Arc;

use axum::routing::{get, post, put};
use axum::{Json, Router};
use conta_verde_application::{
    AuthService, CategoryRepository, CategoryService, CnpjRegistry, CostCenterRepository,
    CostCenterService, IdentityRepository, PartyRepository, PartyService, ProductRepository,
    ProductService, SecretService,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default)]
pub struct ApiConfig {
    /// Adds the `Secure` flag to the session cookie. Turn it on whenever the API is served over HTTPS.
    pub cookie_secure: bool,
}

/// The adapters the use cases run on, named once so that a handler has one type parameter however
/// many registers there are. Generic rather than `dyn`, like the use cases themselves.
pub trait Ports: 'static {
    type Parties: PartyRepository + 'static;
    type Registry: CnpjRegistry + 'static;
    type Identity: IdentityRepository + 'static;
    type Secrets: SecretService + 'static;
    type Products: ProductRepository + 'static;
    type CostCenters: CostCenterRepository + 'static;
    type Categories: CategoryRepository + 'static;
}

/// The use cases the API serves.
pub struct Services<X: Ports> {
    pub parties: PartyService<X::Parties, X::Registry>,
    pub products: ProductService<X::Products>,
    pub cost_centers: CostCenterService<X::CostCenters>,
    pub categories: CategoryService<X::Categories>,
    pub auth: AuthService<X::Identity, X::Secrets>,
}

pub(crate) struct AppState<X: Ports> {
    parties: PartyService<X::Parties, X::Registry>,
    products: ProductService<X::Products>,
    cost_centers: CostCenterService<X::CostCenters>,
    categories: CategoryService<X::Categories>,
    auth: AuthService<X::Identity, X::Secrets>,
    config: ApiConfig,
}

/// What every handler receives.
pub(crate) type SharedState<X> = Arc<AppState<X>>;

pub fn router<X: Ports>(services: Services<X>, config: ApiConfig) -> Router {
    let state = Arc::new(AppState {
        parties: services.parties,
        products: services.products,
        cost_centers: services.cost_centers,
        categories: services.categories,
        auth: services.auth,
        config,
    });
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(auth::register::<X>))
        .route("/auth/login", post(auth::login::<X>))
        .route("/auth/logout", post(auth::logout::<X>))
        .route("/auth/me", get(auth::me::<X>))
        .route(
            "/companies/{company_id}/parties",
            get(parties::list_parties::<X>).post(parties::register_party::<X>),
        )
        .route(
            "/companies/{company_id}/parties/{party_id}/document-check",
            post(parties::verify_document::<X>),
        )
        .route(
            "/companies/{company_id}/parties/{party_id}/history",
            get(parties::party_history::<X>),
        )
        .route(
            "/companies/{company_id}/products",
            get(products::list_products::<X>).post(products::register_product::<X>),
        )
        .route(
            "/companies/{company_id}/products/{product_id}",
            put(products::update_product::<X>),
        )
        .route(
            "/companies/{company_id}/products/{product_id}/history",
            get(products::product_history::<X>),
        )
        .route(
            "/companies/{company_id}/cost-centers",
            get(cost_centers::list_cost_centers::<X>).post(cost_centers::register_cost_center::<X>),
        )
        .route(
            "/companies/{company_id}/cost-centers/{cost_center_id}",
            put(cost_centers::update_cost_center::<X>),
        )
        .route(
            "/companies/{company_id}/cost-centers/{cost_center_id}/history",
            get(cost_centers::cost_center_history::<X>),
        )
        .route(
            "/companies/{company_id}/categories",
            get(categories::list_categories::<X>).post(categories::create_category::<X>),
        )
        .route(
            "/companies/{company_id}/categories/default-template",
            post(categories::apply_default_template::<X>),
        )
        .route(
            "/companies/{company_id}/categories/{category_id}",
            put(categories::change_category::<X>),
        )
        .route(
            "/companies/{company_id}/categories/{category_id}/history",
            get(categories::category_history::<X>),
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
        Argon2SecretService, InMemoryCategoryRepository, InMemoryCnpjRegistry,
        InMemoryCostCenterRepository, InMemoryIdentityRepository, InMemoryPartyRepository,
        InMemoryProductRepository,
    };
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;

    /// The registry knows the one company the tests register, and a closed alphanumeric one.
    fn known_companies() -> InMemoryCnpjRegistry {
        InMemoryCnpjRegistry::default()
            .with_company("11222333000181", "PADARIA CENTRAL LTDA", "ATIVA")
            .with_company("12ABC34501DE35", "OFICINA ALFANUMERICA LTDA", "BAIXADA")
    }

    fn app() -> Router {
        app_with_registry(known_companies())
    }

    /// No waiting between re-checks of a party, so tests can check one right after another.
    fn app_with_registry(registry: InMemoryCnpjRegistry) -> Router {
        app_with_parties(
            PartyService::new(InMemoryPartyRepository::default(), registry)
                .with_recheck_interval(Duration::ZERO),
        )
    }

    struct MemoryPorts;

    impl Ports for MemoryPorts {
        type Parties = InMemoryPartyRepository;
        type Registry = InMemoryCnpjRegistry;
        type Identity = InMemoryIdentityRepository;
        type Secrets = Argon2SecretService;
        type Products = InMemoryProductRepository;
        type CostCenters = InMemoryCostCenterRepository;
        type Categories = InMemoryCategoryRepository;
    }

    /// Like `MemoryPorts`, with the CNPJ lookup switched off.
    struct NoLookupPorts;

    impl Ports for NoLookupPorts {
        type Parties = InMemoryPartyRepository;
        type Registry = Option<InMemoryCnpjRegistry>;
        type Identity = InMemoryIdentityRepository;
        type Secrets = Argon2SecretService;
        type Products = InMemoryProductRepository;
        type CostCenters = InMemoryCostCenterRepository;
        type Categories = InMemoryCategoryRepository;
    }

    fn auth_service() -> AuthService<InMemoryIdentityRepository, Argon2SecretService> {
        AuthService::new(
            InMemoryIdentityRepository::default(),
            Argon2SecretService::new().unwrap(),
            Duration::from_secs(3600),
        )
    }

    fn app_with_parties(
        parties: PartyService<InMemoryPartyRepository, InMemoryCnpjRegistry>,
    ) -> Router {
        router::<MemoryPorts>(
            Services {
                parties,
                products: ProductService::new(InMemoryProductRepository::default()),
                cost_centers: CostCenterService::new(InMemoryCostCenterRepository::default()),
                categories: CategoryService::new(InMemoryCategoryRepository::default()),
                auth: auth_service(),
            },
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

    async fn register_party(app: &Router, document: Option<&str>) -> Reply {
        let (cookie, company_id) = sign_up(app, "ana@example.com").await;
        let mut body = party_body();
        body["document"] = document.map_or(Value::Null, Value::from);
        send(
            app,
            json_request("POST", &parties_uri(company_id), Some(&cookie), body),
        )
        .await
    }

    #[tokio::test]
    async fn a_known_cnpj_is_verified_against_the_registry() {
        let reply = register_party(&app(), Some("11.222.333/0001-81")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(
            reply.body["document_check"],
            json!({
                "status": "verified",
                "legal_name": "PADARIA CENTRAL LTDA",
                "situation": "ATIVA",
                "needs_attention": false
            })
        );
    }

    #[tokio::test]
    async fn an_inactive_company_is_accepted_with_a_warning() {
        let reply = register_party(&app(), Some("12.ABC.345/01DE-35")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(reply.body["document"], "12ABC34501DE35");
        assert_eq!(reply.body["document_check"]["situation"], "BAIXADA");
        assert_eq!(reply.body["document_check"]["needs_attention"], true);
    }

    #[tokio::test]
    async fn a_cnpj_missing_from_the_registry_is_accepted_with_a_warning() {
        // Valid check digits, but unknown to the registry, whose data lags: a new company must
        // still be registrable.
        let reply = register_party(&app(), Some("00.000.000/0001-91")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(reply.body["document"], "00000000000191");
        assert_eq!(reply.body["document_check"]["status"], "not_found");
        assert_eq!(reply.body["document_check"]["needs_attention"], true);
    }

    #[tokio::test]
    async fn an_unreachable_registry_does_not_stop_data_entry() {
        let app = app_with_registry(InMemoryCnpjRegistry::unreachable());
        let reply = register_party(&app, Some("11.222.333/0001-81")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(reply.body["document_check"]["status"], "unverified");
        assert_eq!(reply.body["document_check"]["needs_attention"], true);
    }

    /// Signs up, registers a party with the given document and returns the pieces a follow-up
    /// request needs: the session cookie, the company id and the party id.
    async fn party_of_a_new_user(
        app: &Router,
        email: &str,
        document: &str,
    ) -> (String, Uuid, Uuid) {
        let (cookie, company_id) = sign_up(app, email).await;
        let mut body = party_body();
        body["document"] = Value::from(document);
        let created = send(
            app,
            json_request("POST", &parties_uri(company_id), Some(&cookie), body),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED);
        let party_id = created.body["id"].as_str().unwrap().parse().unwrap();
        (cookie, company_id, party_id)
    }

    fn document_check_uri(company_id: Uuid, party_id: Uuid) -> String {
        format!("{}/{party_id}/document-check", parties_uri(company_id))
    }

    #[tokio::test]
    async fn an_unverified_party_can_be_checked_again() {
        let registry = InMemoryCnpjRegistry::unreachable();
        let app = app_with_registry(registry.clone());
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = document_check_uri(company_id, party_id);

        // The registry is still down: the answer is the same, not an error.
        let still_down = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(still_down.status, StatusCode::OK);
        assert_eq!(still_down.body["document_check"]["status"], "unverified");

        registry.recover();
        registry.add_company("11222333000181", "PADARIA CENTRAL LTDA", "ATIVA");
        let verified = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(verified.status, StatusCode::OK);
        assert_eq!(verified.body["document_check"]["status"], "verified");
        assert_eq!(verified.body["document_check"]["situation"], "ATIVA");

        // The new answer was stored: the list shows it.
        let listed = send(&app, get_request(&parties_uri(company_id), Some(&cookie))).await;
        assert_eq!(
            listed.body["items"][0]["document_check"]["status"],
            "verified"
        );
    }

    #[tokio::test]
    async fn checking_again_needs_a_session_and_the_partys_own_company() {
        let registry = InMemoryCnpjRegistry::unreachable();
        let app = app_with_registry(registry);
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = document_check_uri(company_id, party_id);

        let anonymous = send(&app, json_request("POST", &uri, None, Value::Null)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

        // Another user cannot reach a company they are not a member of...
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let foreign = send(
            &app,
            json_request("POST", &uri, Some(&stranger_cookie), Value::Null),
        )
        .await;
        assert_eq!(foreign.status, StatusCode::NOT_FOUND);

        // ...nor reach a party through a company of their own.
        let through_own_company = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(stranger_company, party_id),
                Some(&stranger_cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(through_own_company.status, StatusCode::NOT_FOUND);

        let unknown_party = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(company_id, Uuid::new_v4()),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(unknown_party.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn checking_again_too_fast_is_answered_with_429() {
        // The default interval: a second re-check right away is refused.
        let app = app_with_parties(PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::unreachable(),
        ));
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = document_check_uri(company_id, party_id);

        let first = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(first.status, StatusCode::OK);
        let second = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(second.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            second.body["error"],
            "this CNPJ was checked a moment ago, try again in a few seconds"
        );
    }

    #[tokio::test]
    async fn checking_again_with_the_lookup_off_is_a_409() {
        let registry: Option<InMemoryCnpjRegistry> = None;
        let app = router::<NoLookupPorts>(
            Services {
                parties: PartyService::new(InMemoryPartyRepository::default(), registry),
                products: ProductService::new(InMemoryProductRepository::default()),
                cost_centers: CostCenterService::new(InMemoryCostCenterRepository::default()),
                categories: CategoryService::new(InMemoryCategoryRepository::default()),
                auth: auth_service(),
            },
            ApiConfig::default(),
        );
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let reply = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(company_id, party_id),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CONFLICT);
        assert_eq!(
            reply.body["error"],
            "CNPJ lookup is switched off on this installation"
        );
    }

    #[tokio::test]
    async fn a_malformed_party_id_gets_the_json_error_like_any_other_miss() {
        let app = app();
        let (cookie, company_id, _) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = format!("{}/not-a-uuid/document-check", parties_uri(company_id));
        let reply = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert_eq!(reply.body["error"], "party not found");
    }

    fn history_uri(company_id: Uuid, party_id: impl std::fmt::Display) -> String {
        format!("{}/{party_id}/history", parties_uri(company_id))
    }

    #[tokio::test]
    async fn the_history_names_who_registered_the_party_and_what_they_set() {
        let app = app();
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;

        let reply = send(
            &app,
            get_request(&history_uri(company_id, party_id), Some(&cookie)),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK);
        let entries = reply.body.as_array().unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry["action"], "created");
        // The name is the one the user signed up with, taken from the session.
        assert_eq!(entry["actor"]["kind"], "user");
        assert_eq!(entry["actor"]["name"], "Ana");
        assert!(entry["actor"]["id"].is_string());
        assert!(entry["at"].as_i64().unwrap() > 1_700_000_000_000);
        let fields: Vec<&str> = entry["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|change| change["field"].as_str().unwrap())
            .collect();
        assert!(fields.contains(&"name") && fields.contains(&"document"));
        assert_eq!(entry["changes"][0]["from"], Value::Null);
        assert_eq!(entry["changes"][0]["to"], "Padaria Central");
    }

    #[tokio::test]
    async fn a_recheck_shows_up_in_the_history_under_whoever_made_it() {
        let registry = InMemoryCnpjRegistry::unreachable();
        let app = app_with_registry(registry.clone());
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;

        registry.recover();
        registry.add_company("11222333000181", "PADARIA CENTRAL LTDA", "ATIVA");
        let checked = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(company_id, party_id),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(checked.status, StatusCode::OK);

        let reply = send(
            &app,
            get_request(&history_uri(company_id, party_id), Some(&cookie)),
        )
        .await;
        let entries = reply.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1]["action"], "updated");
        assert_eq!(entries[1]["changes"][0]["field"], "document_check");
        assert_eq!(entries[1]["changes"][0]["from"], "unverified");
        assert_eq!(entries[1]["changes"][0]["to"], "verified");
        // Oldest first, and no installation-wide counter in the response.
        assert_eq!(entries[0]["action"], "created");
        assert!(entries[0].get("sequence").is_none());
    }

    #[tokio::test]
    async fn the_history_needs_a_session_and_the_partys_own_company() {
        let app = app();
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = history_uri(company_id, party_id);

        let anonymous = send(&app, get_request(&uri, None)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

        // Another user's company, and another company of the same user, both look like a miss.
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let foreign = send(&app, get_request(&uri, Some(&stranger_cookie))).await;
        assert_eq!(foreign.status, StatusCode::NOT_FOUND);
        let through_own_company = send(
            &app,
            get_request(
                &history_uri(stranger_company, party_id),
                Some(&stranger_cookie),
            ),
        )
        .await;
        assert_eq!(through_own_company.status, StatusCode::NOT_FOUND);
        assert_eq!(through_own_company.body["error"], "party not found");

        let malformed = send(
            &app,
            get_request(&history_uri(company_id, "not-a-uuid"), Some(&cookie)),
        )
        .await;
        assert_eq!(malformed.status, StatusCode::NOT_FOUND);
        assert_eq!(malformed.body["error"], "party not found");
    }

    #[tokio::test]
    async fn a_cpf_party_has_nothing_to_check() {
        let app = app();
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "529.982.247-25").await;
        let reply = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(company_id, party_id),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(reply.body["error"], "the party has no CNPJ to verify");
    }

    #[tokio::test]
    async fn a_cpf_is_validated_by_its_digits_only() {
        let app = app_with_registry(InMemoryCnpjRegistry::unreachable());
        let reply = register_party(&app, Some("529.982.247-25")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(reply.body["document_check"]["status"], "not_applicable");
        assert_eq!(reply.body["document_check"]["needs_attention"], false);
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
    async fn filters_and_sorts_the_list_from_query_parameters() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);
        for (name, is_customer, is_supplier) in [
            ("Carla", true, false),
            ("Ana", true, true),
            ("Bruno", false, true),
        ] {
            let body =
                json!({ "name": name, "is_customer": is_customer, "is_supplier": is_supplier });
            send(&app, json_request("POST", &uri, Some(&cookie), body)).await;
        }
        let names = |reply: &Reply| -> Vec<String> {
            reply.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["name"].as_str().unwrap().to_owned())
                .collect()
        };
        let list = |query: &str| {
            let request = get_request(&format!("{uri}?{query}"), Some(&cookie));
            send(&app, request)
        };

        assert_eq!(names(&list("sort=-name").await), ["Carla", "Bruno", "Ana"]);
        assert_eq!(names(&list("kind=supplier").await), ["Ana", "Bruno"]);
        assert_eq!(names(&list("q=AR").await), ["Carla"]);

        // The cursor of a descending page resumes in descending order.
        let first = list("sort=-name&limit=2").await;
        assert_eq!(names(&first), ["Carla", "Bruno"]);
        let cursor = first.body["next_cursor"]
            .as_str()
            .unwrap()
            .replace('|', "%7C");
        let second = list(&format!("sort=-name&limit=2&after={cursor}")).await;
        assert_eq!(names(&second), ["Ana"]);

        for query in [
            "kind=both",
            "kind=",
            "sort=",
            "sort=email",
            "sort=name%20desc",
            &format!("q={}", "a".repeat(101)),
        ] {
            let reply = list(query).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string());
        }
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

    fn products_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/products")
    }

    fn product_body() -> Value {
        json!({
            "kind": "product",
            "name": "Pão francês",
            "sku": "pao-001",
            "unit": "kg",
            "ncm": "1905.90.90",
            "sale_price": "12.50",
            "cost_price": "7",
            "minimum_stock": "10.000",
            "active": true
        })
    }

    async fn post_product(app: &Router, cookie: &str, company_id: Uuid, body: Value) -> Reply {
        let request = json_request("POST", &products_uri(company_id), Some(cookie), body);
        send(app, request).await
    }

    #[tokio::test]
    async fn registers_changes_and_lists_products_with_their_history() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;

        let created = post_product(&app, &cookie, company_id, product_body()).await;
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["sku"], "PAO-001");
        assert_eq!(created.body["unit"], "KG");
        assert_eq!(created.body["ncm"], "19059090");
        // Decimals come back as strings, without trailing zeros.
        assert_eq!(created.body["sale_price"], "12.5");
        assert_eq!(created.body["minimum_stock"], "10");
        assert_eq!(created.body["active"], true);
        let id = created.body["id"].as_str().unwrap().to_owned();

        let mut changed = product_body();
        changed["sale_price"] = json!("15");
        changed["active"] = json!(false);
        let uri = format!("{}/{id}", products_uri(company_id));
        let updated = send(
            &app,
            json_request("PUT", &uri, Some(&cookie), changed.clone()),
        )
        .await;
        assert_eq!(updated.status, StatusCode::OK);
        assert_eq!(updated.body["sale_price"], "15");
        assert_eq!(updated.body["active"], false);

        // The same body again changes nothing and adds nothing to the history.
        let again = send(&app, json_request("PUT", &uri, Some(&cookie), changed)).await;
        assert_eq!(again.status, StatusCode::OK);

        let listed = send(&app, get_request(&products_uri(company_id), Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        let inactive = format!(
            "{}?active=false&kind=product&q=PAO",
            products_uri(company_id)
        );
        let listed = send(&app, get_request(&inactive, Some(&cookie))).await;
        assert_eq!(listed.body["items"][0]["name"], "Pão francês");
        let active = format!("{}?active=true", products_uri(company_id));
        let listed = send(&app, get_request(&active, Some(&cookie))).await;
        assert!(listed.body["items"].as_array().unwrap().is_empty());

        let history = send(&app, get_request(&format!("{uri}/history"), Some(&cookie))).await;
        let entries = history.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["action"], "created");
        assert_eq!(entries[0]["actor"]["name"], "Ana");
        assert_eq!(entries[1]["action"], "updated");
        assert_eq!(
            entries[1]["changes"],
            json!([
                { "field": "sale_price", "from": "12.5", "to": "15" },
                { "field": "active", "from": "true", "to": "false" },
            ])
        );
    }

    #[tokio::test]
    async fn amounts_are_decimal_strings_and_every_refusal_is_json() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let with = |field: &str, value: Value| {
            let mut body = product_body();
            body[field] = value;
            body
        };

        // A JSON number would lose precision on the way: refused, in the API's own error shape.
        for body in [
            with("sale_price", json!(10.5)),
            with("kind", json!("gadget")),
            with("surprise", json!(1)),
            with("sale_price", json!({})),
            json!({ "kind": "product" }),
        ] {
            let reply = post_product(&app, &cookie, company_id, body.clone()).await;
            assert!(reply.status.is_client_error(), "{body}");
            assert!(reply.body["error"].is_string(), "{body}");
        }

        // The domain's own rules are 422 with a message that names the field.
        let comma = post_product(
            &app,
            &cookie,
            company_id,
            with("sale_price", json!("10,50")),
        )
        .await;
        assert_eq!(comma.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            comma.body["error"]
                .as_str()
                .unwrap()
                .starts_with("sale price must be")
        );
        for (field, value) in [
            ("cost_price", json!("-1")),
            ("minimum_stock", json!("1e3")),
            ("unit", json!("quilogramas")),
            ("ncm", json!("123")),
            ("sku", json!("a b")),
            ("name", json!("  ")),
        ] {
            let reply = post_product(&app, &cookie, company_id, with(field, value)).await;
            assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        }
        let service = json!({
            "kind": "service", "name": "Entrega", "unit": "UN", "ncm": "19059090"
        });
        let reply = post_product(&app, &cookie, company_id, service).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn a_sku_is_unique_within_a_company_and_products_stay_private() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;

        let created = post_product(&app, &cookie, company_id, product_body()).await;
        let id = created.body["id"].as_str().unwrap().to_owned();
        let duplicate = post_product(&app, &cookie, company_id, product_body()).await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);
        // Another company may use the same SKU.
        let own = post_product(&app, &stranger_cookie, stranger_company, product_body()).await;
        assert_eq!(own.status, StatusCode::CREATED);

        // Neither the product nor its trail can be reached through another company's route...
        let foreign_uri = format!("{}/{id}", products_uri(stranger_company));
        let put = json_request("PUT", &foreign_uri, Some(&stranger_cookie), product_body());
        assert_eq!(send(&app, put).await.status, StatusCode::NOT_FOUND);
        let history = get_request(&format!("{foreign_uri}/history"), Some(&stranger_cookie));
        assert_eq!(send(&app, history).await.status, StatusCode::NOT_FOUND);
        // ...nor through a company the caller does not belong to.
        let uri = products_uri(company_id);
        let list = send(&app, get_request(&uri, Some(&stranger_cookie))).await;
        assert_eq!(list.status, StatusCode::NOT_FOUND);
        let unknown = format!("{uri}/not-a-uuid");
        let put = json_request("PUT", &unknown, Some(&cookie), product_body());
        let reply = send(&app, put).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert_eq!(reply.body["error"], "product not found");

        let anonymous = send(&app, get_request(&uri, None)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn product_list_parameters_are_checked_and_paged() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        for name in ["Carla", "Ana", "Bruno"] {
            let mut body = product_body();
            body["name"] = json!(name);
            body["sku"] = Value::Null;
            post_product(&app, &cookie, company_id, body).await;
        }
        let uri = products_uri(company_id);
        let first = send(
            &app,
            get_request(&format!("{uri}?limit=2&sort=-name"), Some(&cookie)),
        )
        .await;
        let names: Vec<&str> = first.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Carla", "Bruno"]);
        let cursor = first.body["next_cursor"]
            .as_str()
            .unwrap()
            .replace('|', "%7C");
        let second = send(
            &app,
            get_request(
                &format!("{uri}?limit=2&sort=-name&after={cursor}"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(second.body["items"][0]["name"], "Ana");

        for query in [
            "kind=both",
            "kind=",
            "active=yes",
            "active=",
            "sort=sku",
            "after=garbage",
        ] {
            let reply = send(&app, get_request(&format!("{uri}?{query}"), Some(&cookie))).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }
    }

    #[tokio::test]
    async fn replacing_must_say_whether_the_product_is_active() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let mut inactive = product_body();
        inactive["active"] = json!(false);
        let created = post_product(&app, &cookie, company_id, inactive).await;
        assert_eq!(created.body["active"], false);
        let id = created.body["id"].as_str().unwrap();

        // Registering may leave it out (active), replacing may not: it would switch the product on.
        let uri = format!("{}/{id}", products_uri(company_id));
        let mut without_active = product_body();
        without_active.as_object_mut().unwrap().remove("active");
        let reply = send(
            &app,
            json_request("PUT", &uri, Some(&cookie), without_active.clone()),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("active is required")
        );
        let listed = send(
            &app,
            get_request(
                &format!("{}?active=true", products_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert!(listed.body["items"].as_array().unwrap().is_empty());
        without_active["sku"] = json!("other");
        let bare = post_product(&app, &cookie, company_id, without_active).await;
        assert_eq!(bare.body["active"], true);
    }

    #[tokio::test]
    async fn malformed_list_parameters_and_overlong_names_are_json_refusals() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        for uri in [products_uri(company_id), parties_uri(company_id)] {
            let reply = send(
                &app,
                get_request(&format!("{uri}?limit=abc"), Some(&cookie)),
            )
            .await;
            assert!(reply.status.is_client_error(), "{uri}");
            assert!(reply.body["error"].is_string(), "{uri}");
        }
        let mut long = product_body();
        long["name"] = json!("ã".repeat(201));
        let reply = post_product(&app, &cookie, company_id, long).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("200 characters")
        );
    }

    fn cost_centers_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/cost-centers")
    }

    async fn post_cost_center(app: &Router, cookie: &str, company_id: Uuid, body: Value) -> Reply {
        let request = json_request("POST", &cost_centers_uri(company_id), Some(cookie), body);
        send(app, request).await
    }

    #[tokio::test]
    async fn registers_changes_and_lists_cost_centers_with_their_history() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;

        let created = post_cost_center(
            &app,
            &cookie,
            company_id,
            json!({ "name": "  Loja Centro " }),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["name"], "Loja Centro");
        assert_eq!(created.body["active"], true);
        let id = created.body["id"].as_str().unwrap().to_owned();

        // The same name in another case is the same cost center.
        let duplicate =
            post_cost_center(&app, &cookie, company_id, json!({ "name": "LOJA centro" })).await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);

        let uri = format!("{}/{id}", cost_centers_uri(company_id));
        let changed = json!({ "name": "Loja Norte", "active": false });
        let updated = send(
            &app,
            json_request("PUT", &uri, Some(&cookie), changed.clone()),
        )
        .await;
        assert_eq!(updated.status, StatusCode::OK);
        assert_eq!(updated.body["active"], false);
        // The same body again changes nothing and adds nothing to the history.
        send(&app, json_request("PUT", &uri, Some(&cookie), changed)).await;

        let inactive = format!("{}?active=false&q=NORTE", cost_centers_uri(company_id));
        let listed = send(&app, get_request(&inactive, Some(&cookie))).await;
        assert_eq!(listed.body["items"][0]["name"], "Loja Norte");

        let history = send(&app, get_request(&format!("{uri}/history"), Some(&cookie))).await;
        let entries = history.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["action"], "created");
        assert_eq!(entries[0]["actor"]["name"], "Ana");
        assert_eq!(
            entries[1]["changes"],
            json!([
                { "field": "name", "from": "Loja Centro", "to": "Loja Norte" },
                { "field": "active", "from": "true", "to": "false" },
            ])
        );

        // Replacing must say whether it is active, or a deactivated one would come back.
        let without = json_request("PUT", &uri, Some(&cookie), json!({ "name": "Loja Norte" }));
        assert_eq!(
            send(&app, without).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn cost_centers_stay_private_and_refusals_are_json() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let created = post_cost_center(&app, &cookie, company_id, json!({ "name": "Loja" })).await;
        let id = created.body["id"].as_str().unwrap().to_owned();

        // The same name is fine in another company; its routes cannot reach this one's.
        let own = post_cost_center(
            &app,
            &stranger_cookie,
            stranger_company,
            json!({ "name": "Loja" }),
        )
        .await;
        assert_eq!(own.status, StatusCode::CREATED);
        let foreign = format!("{}/{id}", cost_centers_uri(stranger_company));
        let put = json_request(
            "PUT",
            &foreign,
            Some(&stranger_cookie),
            json!({ "name": "X", "active": true }),
        );
        assert_eq!(send(&app, put).await.status, StatusCode::NOT_FOUND);
        let history = get_request(&format!("{foreign}/history"), Some(&stranger_cookie));
        assert_eq!(send(&app, history).await.status, StatusCode::NOT_FOUND);
        let list = get_request(&cost_centers_uri(company_id), Some(&stranger_cookie));
        assert_eq!(send(&app, list).await.status, StatusCode::NOT_FOUND);
        let anonymous = get_request(&cost_centers_uri(company_id), None);
        assert_eq!(send(&app, anonymous).await.status, StatusCode::UNAUTHORIZED);

        let malformed = format!("{}/not-a-uuid", cost_centers_uri(company_id));
        let put = json_request(
            "PUT",
            &malformed,
            Some(&cookie),
            json!({ "name": "X", "active": true }),
        );
        let reply = send(&app, put).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert_eq!(reply.body["error"], "cost center not found");

        for body in [
            json!({}),
            json!({ "name": "X", "surprise": 1 }),
            json!({ "name": 5 }),
        ] {
            let reply = post_cost_center(&app, &cookie, company_id, body.clone()).await;
            assert!(reply.status.is_client_error(), "{body}");
            assert!(reply.body["error"].is_string(), "{body}");
        }
        let blank = post_cost_center(&app, &cookie, company_id, json!({ "name": "  " })).await;
        assert_eq!(blank.status, StatusCode::UNPROCESSABLE_ENTITY);
        let long = post_cost_center(
            &app,
            &cookie,
            company_id,
            json!({ "name": "x".repeat(201) }),
        )
        .await;
        assert_eq!(long.status, StatusCode::UNPROCESSABLE_ENTITY);

        let uri = cost_centers_uri(company_id);
        for query in [
            "active=yes",
            "active=",
            "sort=id",
            "limit=abc",
            "after=garbage",
        ] {
            let reply = send(&app, get_request(&format!("{uri}?{query}"), Some(&cookie))).await;
            assert!(reply.status.is_client_error(), "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }
    }

    fn categories_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/categories")
    }

    async fn post_category(app: &Router, cookie: &str, company_id: Uuid, body: Value) -> Reply {
        let request = json_request("POST", &categories_uri(company_id), Some(cookie), body);
        send(app, request).await
    }

    #[tokio::test]
    async fn builds_a_tree_of_categories_whose_groups_come_from_the_root() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;

        let root = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Despesas", "dre_group": "operating_expenses", "cash_flow_group": "operating" }),
        )
        .await;
        assert_eq!(root.status, StatusCode::CREATED);
        assert_eq!(root.body["parent_id"], Value::Null);
        assert_eq!(root.body["level"], 1);
        let root_id = root.body["id"].as_str().unwrap().to_owned();

        let child = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Aluguel", "parent_id": root_id }),
        )
        .await;
        assert_eq!(child.status, StatusCode::CREATED);
        assert_eq!(child.body["parent_id"], root_id.as_str());
        assert_eq!(child.body["dre_group"], "operating_expenses");
        assert_eq!(child.body["level"], 2);
        let child_id = child.body["id"].as_str().unwrap().to_owned();

        // The groups of a subcategory are not its own to choose, and a root has to choose them.
        let with_groups = json!({ "name": "X", "parent_id": root_id, "dre_group": "costs", "cash_flow_group": "operating" });
        assert_eq!(
            post_category(&app, &cookie, company_id, with_groups)
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let bare_root = post_category(&app, &cookie, company_id, json!({ "name": "Y" })).await;
        assert_eq!(bare_root.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            bare_root.body["error"]
                .as_str()
                .unwrap()
                .contains("needs a DRE group")
        );
        let duplicate = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "ALUGUEL", "parent_id": root_id }),
        )
        .await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);

        // The whole tree, flat and in a fixed order; the active ones can be asked for alone.
        let listed = send(
            &app,
            get_request(&categories_uri(company_id), Some(&cookie)),
        )
        .await;
        let names: Vec<&str> = listed.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Aluguel", "Despesas"]);

        // Switched off from the bottom up; a body that moves it or changes its groups is refused.
        let uri = |id: &str| format!("{}/{id}", categories_uri(company_id));
        let off = |name: &str| json!({ "name": name, "active": false });
        let blocked = send(
            &app,
            json_request("PUT", &uri(&root_id), Some(&cookie), off("Despesas")),
        )
        .await;
        assert_eq!(blocked.status, StatusCode::CONFLICT);
        let leaf = send(
            &app,
            json_request("PUT", &uri(&child_id), Some(&cookie), off("Aluguel")),
        )
        .await;
        assert_eq!(leaf.status, StatusCode::OK);
        assert_eq!(leaf.body["active"], false);
        let moved = json!({ "name": "Aluguel", "active": true, "parent_id": root_id });
        assert!(
            send(
                &app,
                json_request("PUT", &uri(&child_id), Some(&cookie), moved)
            )
            .await
            .status
            .is_client_error()
        );
        let regrouped = json!({ "name": "Aluguel", "active": true, "dre_group": "costs" });
        assert!(
            send(
                &app,
                json_request("PUT", &uri(&child_id), Some(&cookie), regrouped)
            )
            .await
            .status
            .is_client_error()
        );
        let without_active = json_request(
            "PUT",
            &uri(&child_id),
            Some(&cookie),
            json!({ "name": "Aluguel" }),
        );
        assert_eq!(
            send(&app, without_active).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let active_only = format!("{}?active=true", categories_uri(company_id));
        let listed = send(&app, get_request(&active_only, Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);

        // Under an inactive parent nothing active can go.
        let off_root = send(
            &app,
            json_request("PUT", &uri(&root_id), Some(&cookie), off("Despesas")),
        )
        .await;
        assert_eq!(off_root.status, StatusCode::OK);
        let under = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Luz", "parent_id": root_id }),
        )
        .await;
        assert_eq!(under.status, StatusCode::CONFLICT);
        let unknown = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Luz", "parent_id": Uuid::new_v4() }),
        )
        .await;
        assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);

        let history = send(
            &app,
            get_request(&format!("{}/history", uri(&child_id)), Some(&cookie)),
        )
        .await;
        let entries = history.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[1]["changes"],
            json!([{ "field": "active", "from": "true", "to": "false" }])
        );
    }

    #[tokio::test]
    async fn the_default_template_is_applied_once_and_the_transfer_category_is_kept() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let template = format!("{}/default-template", categories_uri(company_id));

        let applied = send(
            &app,
            json_request("POST", &template, Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(applied.status, StatusCode::CREATED);
        let items = applied.body["items"].as_array().unwrap();
        assert!(items.len() > 20);
        let transfers: Vec<&Value> = items
            .iter()
            .filter(|item| item["dre_group"] == "transfers")
            .collect();
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0]["cash_flow_group"], "transfer");
        assert_eq!(transfers[0]["parent_id"], Value::Null);
        let again = send(
            &app,
            json_request("POST", &template, Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(again.status, StatusCode::CONFLICT);

        // It can be renamed but not switched off, and it cannot be duplicated.
        let uri = format!(
            "{}/{}",
            categories_uri(company_id),
            transfers[0]["id"].as_str().unwrap()
        );
        let off = json_request(
            "PUT",
            &uri,
            Some(&cookie),
            json!({ "name": "Transferências", "active": false }),
        );
        assert_eq!(
            send(&app, off).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let renamed = json_request(
            "PUT",
            &uri,
            Some(&cookie),
            json!({ "name": "Entre contas", "active": true }),
        );
        assert_eq!(send(&app, renamed).await.body["dre_group"], "transfers");
        let second = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Outra", "dre_group": "transfers", "cash_flow_group": "transfer" }),
        )
        .await;
        assert_eq!(second.status, StatusCode::CONFLICT);
        let mismatched = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Outra", "dre_group": "transfers", "cash_flow_group": "operating" }),
        )
        .await;
        assert_eq!(mismatched.status, StatusCode::UNPROCESSABLE_ENTITY);
        let unknown_group = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Outra", "dre_group": "profit", "cash_flow_group": "operating" }),
        )
        .await;
        assert_eq!(unknown_group.status, StatusCode::UNPROCESSABLE_ENTITY);

        // Another company sees none of it, cannot change it, and starts from nothing.
        let theirs = send(
            &app,
            get_request(&categories_uri(stranger_company), Some(&stranger_cookie)),
        )
        .await;
        assert!(theirs.body["items"].as_array().unwrap().is_empty());
        let foreign = json_request(
            "PUT",
            &format!(
                "{}/{}",
                categories_uri(stranger_company),
                transfers[0]["id"].as_str().unwrap()
            ),
            Some(&stranger_cookie),
            json!({ "name": "X", "active": true }),
        );
        assert_eq!(send(&app, foreign).await.status, StatusCode::NOT_FOUND);
        let not_theirs = send(
            &app,
            get_request(&categories_uri(company_id), Some(&stranger_cookie)),
        )
        .await;
        assert_eq!(not_theirs.status, StatusCode::NOT_FOUND);
        let anonymous = send(&app, json_request("POST", &template, None, Value::Null)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
        let malformed = json_request(
            "PUT",
            &format!("{}/not-a-uuid", categories_uri(company_id)),
            Some(&cookie),
            json!({ "name": "X", "active": true }),
        );
        let reply = send(&app, malformed).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert_eq!(reply.body["error"], "category not found");
        for query in ["active=yes", "active=", "active=maybe"] {
            let uri = format!("{}?{query}", categories_uri(company_id));
            let reply = send(&app, get_request(&uri, Some(&cookie))).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }
    }
}
