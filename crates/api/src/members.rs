use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use conta_verde_application::{InvitationPreview, Invited, Member};
use conta_verde_domain::{Invitation, InvitationId, Role};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::json::ApiJson;
use crate::session::require_token;
use crate::{Ports, SharedState};

#[derive(Serialize)]
pub(crate) struct MemberResponse {
    user_id: Uuid,
    name: String,
    email: String,
    role: &'static str,
}

impl From<&Member> for MemberResponse {
    fn from(member: &Member) -> Self {
        Self {
            user_id: member.user.id().as_uuid(),
            name: member.user.name().to_owned(),
            email: member.user.email().as_str().to_owned(),
            role: member.role.as_str(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct InvitationResponse {
    id: Uuid,
    email: String,
    role: &'static str,
    /// Milliseconds since the Unix epoch.
    created_at: i64,
    expires_at: i64,
    expired: bool,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

impl From<&Invitation> for InvitationResponse {
    fn from(invitation: &Invitation) -> Self {
        Self {
            id: invitation.id().as_uuid(),
            email: invitation.email().as_str().to_owned(),
            role: invitation.role().as_str(),
            created_at: invitation.created_at_millis(),
            expires_at: invitation.expires_at_millis(),
            expired: invitation.is_expired_at(now_millis()),
        }
    }
}

/// An invitation just made: the same, and the link that accepts it, which is shown this once.
#[derive(Serialize)]
pub(crate) struct InvitedResponse {
    #[serde(flatten)]
    invitation: InvitationResponse,
    link: String,
    /// Whether the e-mail with the link went out. The invitation exists either way.
    emailed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InviteRequest {
    email: String,
    role: String,
}

pub(crate) async fn list_members<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Vec<MemberResponse>>, ApiError> {
    let members = state.members.members(company_id).await?;
    Ok(Json(members.iter().map(MemberResponse::from).collect()))
}

pub(crate) async fn list_invitations<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Vec<InvitationResponse>>, ApiError> {
    let open = state.members.open_invitations(company_id).await?;
    Ok(Json(open.iter().map(InvitationResponse::from).collect()))
}

pub(crate) async fn invite<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<InviteRequest>,
) -> Result<(StatusCode, Json<InvitedResponse>), ApiError> {
    let role = Role::parse(&request.role).ok_or(ApiError::InvalidField(
        "role must be admin, financial, sales, stock, accountant or viewer",
    ))?;
    let Invited {
        invitation,
        link,
        emailed,
    } = state
        .members
        .invite(company_id, &actor, &request.email, role)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(InvitedResponse {
            invitation: InvitationResponse::from(&invitation),
            link,
            emailed,
        }),
    ))
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API.
fn parse_invitation_id(raw: &str) -> Result<InvitationId, ApiError> {
    Uuid::parse_str(raw)
        .map(InvitationId::from_uuid)
        .map_err(|_| conta_verde_application::ApplicationError::InvitationNotFound.into())
}

pub(crate) async fn revoke_invitation<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, invitation_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .members
        .revoke(company_id, parse_invitation_id(&invitation_id)?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub(crate) struct PreviewResponse {
    company_name: String,
    email: String,
    role: &'static str,
    expires_at: i64,
}

impl From<InvitationPreview> for PreviewResponse {
    fn from(preview: InvitationPreview) -> Self {
        Self {
            company_name: preview.company_name,
            email: preview.email.as_str().to_owned(),
            role: preview.role.as_str(),
            expires_at: preview.expires_at_millis,
        }
    }
}

/// What a link leads to, for the page that asks the person to accept. It needs no session: the
/// person may not have an account yet, and the token is the secret.
pub(crate) async fn preview_invitation<X: Ports>(
    State(state): State<SharedState<X>>,
    Path(token): Path<String>,
) -> Result<Json<PreviewResponse>, ApiError> {
    Ok(Json(PreviewResponse::from(
        state.members.preview(&token).await?,
    )))
}

#[derive(Serialize)]
pub(crate) struct AcceptedResponse {
    company_id: Uuid,
}

/// Makes the signed-in user a member of the invitation's company. It is not a company route (the
/// caller is not a member yet), so it asks for a session itself, and the use case checks that the
/// session is the address the invitation was sent to.
pub(crate) async fn accept_invitation<X: Ports>(
    State(state): State<SharedState<X>>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> Result<Json<AcceptedResponse>, ApiError> {
    let user = state.auth.current_user(require_token(&headers)?).await?;
    let company_id = state.members.accept(&token, &user).await?;
    Ok(Json(AcceptedResponse {
        company_id: company_id.as_uuid(),
    }))
}
