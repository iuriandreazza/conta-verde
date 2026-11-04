use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use conta_verde_application::{ApplicationError, PartyRepository, PartyService};
use conta_verde_domain::{CompanyId, Party, PartyDraft};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router<R>(service: Arc<PartyService<R>>) -> Router
where
    R: PartyRepository + 'static,
{
    Router::new()
        .route("/health", get(health))
        .route(
            "/companies/{company_id}/parties",
            get(list_parties::<R>).post(register_party::<R>),
        )
        .with_state(service)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[derive(Deserialize)]
struct RegisterPartyRequest {
    name: String,
    document: Option<String>,
    email: Option<String>,
    is_customer: bool,
    is_supplier: bool,
}

impl From<RegisterPartyRequest> for PartyDraft {
    fn from(request: RegisterPartyRequest) -> Self {
        Self {
            name: request.name,
            document: request.document,
            email: request.email,
            is_customer: request.is_customer,
            is_supplier: request.is_supplier,
        }
    }
}

#[derive(Serialize)]
struct PartyResponse {
    id: Uuid,
    company_id: Uuid,
    name: String,
    document: Option<String>,
    email: Option<String>,
    is_customer: bool,
    is_supplier: bool,
}

impl From<&Party> for PartyResponse {
    fn from(party: &Party) -> Self {
        Self {
            id: party.id().as_uuid(),
            company_id: party.company_id().as_uuid(),
            name: party.name().to_owned(),
            document: party
                .document()
                .map(|document| document.digits().to_owned()),
            email: party.email().map(str::to_owned),
            is_customer: party.is_customer(),
            is_supplier: party.is_supplier(),
        }
    }
}

async fn register_party<R: PartyRepository>(
    State(service): State<Arc<PartyService<R>>>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<RegisterPartyRequest>,
) -> Result<(StatusCode, Json<PartyResponse>), ApiError> {
    let party = service
        .register(CompanyId::from_uuid(company_id), request.into())
        .await?;
    Ok((StatusCode::CREATED, Json(PartyResponse::from(&party))))
}

async fn list_parties<R: PartyRepository>(
    State(service): State<Arc<PartyService<R>>>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<PartyResponse>>, ApiError> {
    let parties = service.list(CompanyId::from_uuid(company_id)).await?;
    Ok(Json(parties.iter().map(PartyResponse::from).collect()))
}

struct ApiError(ApplicationError);

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        Self(error)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ApplicationError::Domain(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApplicationError::DuplicateDocument => StatusCode::CONFLICT,
            ApplicationError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // Storage details can leak internals, so clients only see a generic message for them.
        let message = match self.0 {
            ApplicationError::Storage(_) => "internal error".to_owned(),
            ref error => error.to_string(),
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, header};
    use conta_verde_infrastructure::InMemoryPartyRepository;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use super::*;

    fn app() -> Router {
        router(Arc::new(PartyService::new(
            InMemoryPartyRepository::default(),
        )))
    }

    async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn post_party(company_id: Uuid, body: Value) -> Request<Body> {
        Request::post(format!("/companies/{company_id}/parties"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn valid_body() -> Value {
        json!({
            "name": "Padaria Central",
            "document": "11.222.333/0001-81",
            "is_customer": true,
            "is_supplier": false
        })
    }

    #[tokio::test]
    async fn health_reports_ok() {
        let (status, body) =
            send(&app(), Request::get("/health").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn registers_then_lists_a_party() {
        let app = app();
        let company_id = Uuid::new_v4();

        let (status, created) = send(&app, post_party(company_id, valid_body())).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(created["document"], "11222333000181");

        let list_request = Request::get(format!("/companies/{company_id}/parties"))
            .body(Body::empty())
            .unwrap();
        let (status, listed) = send(&app, list_request).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn invalid_party_is_unprocessable() {
        let body = json!({ "name": "", "is_customer": true, "is_supplier": false });
        let (status, error) = send(&app(), post_party(Uuid::new_v4(), body)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(error["error"], "name must not be empty");
    }

    #[tokio::test]
    async fn duplicate_document_conflicts() {
        let app = app();
        let company_id = Uuid::new_v4();
        send(&app, post_party(company_id, valid_body())).await;

        let (status, _) = send(&app, post_party(company_id, valid_body())).await;
        assert_eq!(status, StatusCode::CONFLICT);
    }
}
