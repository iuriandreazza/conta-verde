use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use conta_verde_application::{ApplicationError, Profile, RepositoryError, SignedIn};
use conta_verde_domain::{AccountDraft, Area};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// The first company of the account. Left out when signing up through an invitation, which
    /// brings the account into the invited company instead, and required otherwise.
    company_name: Option<String>,
    /// The token of an invitation's link, for the address the account is made for.
    invitation: Option<String>,
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
    /// Whether the address was confirmed; the web app asks for it until it is.
    email_verified: bool,
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
    /// What the caller is in this company: `owner`, `admin`, `financial`, `sales`, `stock`,
    /// `accountant` or `viewer`.
    role: &'static str,
    /// How far the role reaches into each area (`parties`, `products`, `finance`, `members`):
    /// `read` or `write`, and an area the role cannot reach is left out. The web app reads this
    /// instead of knowing the table.
    permissions: BTreeMap<&'static str, &'static str>,
}

impl From<&Profile> for ProfileResponse {
    fn from(profile: &Profile) -> Self {
        Self {
            user: UserResponse {
                id: profile.user.id().as_uuid(),
                name: profile.user.name().to_owned(),
                email: profile.user.email().as_str().to_owned(),
            },
            email_verified: profile.email_verified,
            companies: profile
                .memberships
                .iter()
                .map(|membership| CompanyResponse {
                    id: membership.company.id().as_uuid(),
                    name: membership.company.name().to_owned(),
                    role: membership.role.as_str(),
                    permissions: Area::ALL
                        .into_iter()
                        .filter_map(|area| {
                            membership
                                .role
                                .access_to(area)
                                .map(|access| (area.as_str(), access.as_str()))
                        })
                        .collect(),
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
    let signed_in = match request.invitation {
        // Joining a company that was made by another: no company of one's own is made, and the
        // address is the one the invitation was sent to ([ADR 0027]).
        Some(token) => {
            if request.company_name.is_some() {
                return Err(ApiError::InvalidField(
                    "company_name is not used when signing up through an invitation: the account joins the invited company",
                ));
            }
            let (user, _) = state
                .members
                .join(&token, &request.name, &request.email, &request.password)
                .await?;
            state.auth.sign_in_new_member(user).await?
        }
        None => {
            let company_name = request
                .company_name
                .ok_or(ApiError::InvalidField("company_name is required"))?;
            let email = request.email.clone();
            let draft = AccountDraft {
                user_name: request.name,
                email: request.email,
                company_name,
            };
            let registered = state.auth.register(draft, &request.password).await;
            // The address belongs to an account nobody confirmed: its real owner is sent the way to take it over.
            if matches!(
                registered,
                Err(ApplicationError::Repository(
                    RepositoryError::DuplicateEmail
                ))
            ) && state.members.recover_unconfirmed_address(&email).await?
            {
                return Err(ApplicationError::AddressNeverConfirmed.into());
            }
            registered?
        }
    };
    // A person who made a company of their own is asked to confirm the address; one who joined by an invitation just did.
    // Failing to queue the e-mail never fails the sign-up: the page offers to send it again.
    let _ = state
        .members
        .send_email_confirmation(&signed_in.profile.user)
        .await;
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

#[derive(Deserialize)]
pub(crate) struct PasswordResetRequest {
    email: String,
}

/// Asks for a link to choose a new password. It answers `202` for any address, so it tells nobody who has an account;
/// the link is only in the e-mail.
pub(crate) async fn request_password_reset<X: Ports>(
    State(state): State<SharedState<X>>,
    ApiJson(request): ApiJson<PasswordResetRequest>,
) -> Result<StatusCode, ApiError> {
    state.members.request_password_reset(&request.email).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
pub(crate) struct ResetPasswordRequest {
    token: String,
    password: String,
}

/// Chooses a new password with the token of the e-mailed link; the link works once and every session ends.
pub(crate) async fn reset_password<X: Ports>(
    State(state): State<SharedState<X>>,
    ApiJson(request): ApiJson<ResetPasswordRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .members
        .reset_password(&request.token, &request.password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Sends the signed-in user a new link to confirm their address (`202` whether or not one was made: one a minute at most,
/// and none when it is confirmed already).
pub(crate) async fn request_email_confirmation<X: Ports>(
    State(state): State<SharedState<X>>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = state.auth.current_user(require_token(&headers)?).await?;
    state.members.send_email_confirmation(&user).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
pub(crate) struct ConfirmEmailRequest {
    token: String,
}

/// Confirms the address with the token of the e-mailed link; no session, since the token is the proof.
pub(crate) async fn confirm_email<X: Ports>(
    State(state): State<SharedState<X>>,
    ApiJson(request): ApiJson<ConfirmEmailRequest>,
) -> Result<StatusCode, ApiError> {
    state.members.confirm_email(&request.token).await?;
    Ok(StatusCode::NO_CONTENT)
}
