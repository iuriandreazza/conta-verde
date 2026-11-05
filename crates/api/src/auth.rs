use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use conta_verde_application::{Profile, SignedIn};
use conta_verde_domain::AccountDraft;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;
use crate::json::ApiJson;
use crate::session::{clearing_cookie, require_token, session_cookie, token_from};
use crate::{AppState, Ports, SharedState};

#[derive(Deserialize)]
pub(crate) struct RegisterRequest {
    name: String,
    email: String,
    password: String,
    company_name: String,
}

#[derive(Deserialize)]
pub(crate) struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
pub(crate) struct ProfileResponse {
    user: UserResponse,
    companies: Vec<CompanyResponse>,
}

#[derive(Serialize)]
struct UserResponse {
    id: Uuid,
    name: String,
    email: String,
}

#[derive(Serialize)]
struct CompanyResponse {
    id: Uuid,
    name: String,
}

impl From<&Profile> for ProfileResponse {
    fn from(profile: &Profile) -> Self {
        Self {
            user: UserResponse {
                id: profile.user.id().as_uuid(),
                name: profile.user.name().to_owned(),
                email: profile.user.email().as_str().to_owned(),
            },
            companies: profile
                .companies
                .iter()
                .map(|company| CompanyResponse {
                    id: company.id().as_uuid(),
                    name: company.name().to_owned(),
                })
                .collect(),
        }
    }
}

type SetCookie = [(header::HeaderName, String); 1];

pub(crate) async fn register<X: Ports>(
    State(state): State<SharedState<X>>,
    ApiJson(request): ApiJson<RegisterRequest>,
) -> Result<(StatusCode, SetCookie, Json<ProfileResponse>), ApiError> {
    let draft = AccountDraft {
        user_name: request.name,
        email: request.email,
        company_name: request.company_name,
    };
    let signed_in = state.auth.register(draft, &request.password).await?;
    Ok((
        StatusCode::CREATED,
        started(&state, &signed_in),
        body(&signed_in),
    ))
}

pub(crate) async fn login<X: Ports>(
    State(state): State<SharedState<X>>,
    ApiJson(request): ApiJson<LoginRequest>,
) -> Result<(SetCookie, Json<ProfileResponse>), ApiError> {
    let signed_in = state.auth.login(&request.email, &request.password).await?;
    Ok((started(&state, &signed_in), body(&signed_in)))
}

pub(crate) async fn logout<X: Ports>(
    State(state): State<SharedState<X>>,
    headers: HeaderMap,
) -> Result<(StatusCode, SetCookie), ApiError> {
    if let Some(token) = token_from(&headers) {
        state.auth.logout(token).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            clearing_cookie(state.config.cookie_secure),
        )],
    ))
}

pub(crate) async fn me<X: Ports>(
    State(state): State<SharedState<X>>,
    headers: HeaderMap,
) -> Result<Json<ProfileResponse>, ApiError> {
    let token = require_token(&headers)?;
    let profile = state.auth.profile(token).await?;
    Ok(Json(ProfileResponse::from(&profile)))
}

fn started<X: Ports>(state: &AppState<X>, signed_in: &SignedIn) -> SetCookie {
    let cookie = session_cookie(
        &signed_in.token,
        state.auth.session_lifetime(),
        state.config.cookie_secure,
    );
    [(header::SET_COOKIE, cookie)]
}

fn body(signed_in: &SignedIn) -> Json<ProfileResponse> {
    Json(ProfileResponse::from(&signed_in.profile))
}
