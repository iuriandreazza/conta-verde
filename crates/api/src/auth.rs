use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use conta_verde_application::{
    IdentityRepository, PartyRepository, Profile, SecretService, SignedIn,
};
use conta_verde_domain::AccountDraft;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::error::ApiError;
use crate::session::{clearing_cookie, require_token, session_cookie, token_from};

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

pub(crate) async fn register<R, I, S>(
    State(state): State<Arc<AppState<R, I, S>>>,
    Json(request): Json<RegisterRequest>,
) -> Result<(StatusCode, SetCookie, Json<ProfileResponse>), ApiError>
where
    R: PartyRepository,
    I: IdentityRepository,
    S: SecretService,
{
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

pub(crate) async fn login<R, I, S>(
    State(state): State<Arc<AppState<R, I, S>>>,
    Json(request): Json<LoginRequest>,
) -> Result<(SetCookie, Json<ProfileResponse>), ApiError>
where
    R: PartyRepository,
    I: IdentityRepository,
    S: SecretService,
{
    let signed_in = state.auth.login(&request.email, &request.password).await?;
    Ok((started(&state, &signed_in), body(&signed_in)))
}

pub(crate) async fn logout<R, I, S>(
    State(state): State<Arc<AppState<R, I, S>>>,
    headers: HeaderMap,
) -> Result<(StatusCode, SetCookie), ApiError>
where
    R: PartyRepository,
    I: IdentityRepository,
    S: SecretService,
{
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

pub(crate) async fn me<R, I, S>(
    State(state): State<Arc<AppState<R, I, S>>>,
    headers: HeaderMap,
) -> Result<Json<ProfileResponse>, ApiError>
where
    R: PartyRepository,
    I: IdentityRepository,
    S: SecretService,
{
    let token = require_token(&headers)?;
    let profile = state.auth.profile(token).await?;
    Ok(Json(ProfileResponse::from(&profile)))
}

fn started<R, I, S>(state: &AppState<R, I, S>, signed_in: &SignedIn) -> SetCookie
where
    R: PartyRepository,
    I: IdentityRepository,
    S: SecretService,
{
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
