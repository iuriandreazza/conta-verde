use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use conta_verde_application::ApplicationError;
use conta_verde_domain::{Attachment, AttachmentId, EntryId, MAX_ATTACHMENT_BYTES};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::csv_import::read_body;
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

/// The largest body read: the file, which the domain then measures against its own limit.
const MAX_BODY_BYTES: usize = MAX_ATTACHMENT_BYTES as usize;

#[derive(Serialize)]
pub(crate) struct AttachmentResponse {
    id: Uuid,
    entry_id: Uuid,
    file_name: String,
    content_type: String,
    /// In bytes.
    size: u64,
}

impl From<&Attachment> for AttachmentResponse {
    fn from(attachment: &Attachment) -> Self {
        Self {
            id: attachment.id().as_uuid(),
            entry_id: attachment.entry_id().as_uuid(),
            file_name: attachment.file_name().to_owned(),
            content_type: attachment.content_type().to_owned(),
            size: attachment.size(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct AttachmentListResponse {
    items: Vec<AttachmentResponse>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttachQuery {
    /// The file's name, as the person knows it.
    name: String,
}

// Strings, parsed here, like the other ids: axum's own rejection is plain text.
fn parse_entry(raw: &str) -> Result<EntryId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntryId::from_uuid)
        .map_err(|_| ApplicationError::EntryNotFound.into())
}

fn parse_attachment(raw: &str) -> Result<AttachmentId, ApiError> {
    Uuid::parse_str(raw)
        .map(AttachmentId::from_uuid)
        .map_err(|_| ApplicationError::AttachmentNotFound.into())
}

/// What the browser said the file is, without parameters such as `; charset=utf-8`.
fn content_type_of(headers: &HeaderMap) -> &str {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .unwrap_or_default()
        .trim()
}

pub(crate) async fn list_attachments<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, entry_id)): Path<(Uuid, String)>,
) -> Result<Json<AttachmentListResponse>, ApiError> {
    let attachments = state
        .attachments
        .list(company_id, parse_entry(&entry_id)?)
        .await?;
    Ok(Json(AttachmentListResponse {
        items: attachments.iter().map(AttachmentResponse::from).collect(),
    }))
}

/// The file is the body of the request, as it is, and its type is the `Content-Type`: no multipart to
/// parse, and the name travels in the query.
pub(crate) async fn attach<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, entry_id)): Path<(Uuid, String)>,
    ApiQuery(query): ApiQuery<AttachQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<(StatusCode, Json<AttachmentResponse>), ApiError> {
    let entry_id = parse_entry(&entry_id)?;
    let bytes = read_body(
        &headers,
        body,
        MAX_BODY_BYTES,
        "the file is larger than 10 MB",
    )
    .await?;
    let attachment = state
        .attachments
        .attach(
            company_id,
            &actor,
            entry_id,
            &query.name,
            content_type_of(&headers),
            &bytes,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(AttachmentResponse::from(&attachment)),
    ))
}

/// Always as a download, with the type it was attached as and no sniffing: a file is never shown as a
/// page of the app's own origin.
pub(crate) async fn download_attachment<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, entry_id, attachment_id)): Path<(Uuid, String, String)>,
) -> Result<Response, ApiError> {
    let file = state
        .attachments
        .download(
            company_id,
            parse_entry(&entry_id)?,
            parse_attachment(&attachment_id)?,
        )
        .await?;
    let mut response = file.bytes.into_response();
    let headers = response.headers_mut();
    let attachment = &file.attachment;
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(attachment.content_type())
            .expect("a content type of the allowed list is a header value"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition(attachment.file_name()))
            .expect("a percent-encoded name is a header value"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

pub(crate) async fn remove_attachment<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, entry_id, attachment_id)): Path<(Uuid, String, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .attachments
        .remove(
            company_id,
            &actor,
            parse_entry(&entry_id)?,
            parse_attachment(&attachment_id)?,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `attachment` with the name twice: a plain one for old clients, with what is not ASCII or not safe in
/// a quoted string turned into `_`, and the real one, percent-encoded as UTF-8 (RFC 6266).
fn content_disposition(file_name: &str) -> String {
    let plain: String = file_name
        .chars()
        .map(|c| {
            if (c.is_ascii_graphic() && !matches!(c, '"' | '\\' | '%')) || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::new();
    for byte in file_name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("attachment; filename=\"{plain}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_is_offered_plain_and_encoded() {
        assert_eq!(
            content_disposition("Nota 123.pdf"),
            "attachment; filename=\"Nota 123.pdf\"; filename*=UTF-8''Nota%20123.pdf"
        );
        assert_eq!(
            content_disposition("Recibo \"ç\"%.png"),
            "attachment; filename=\"Recibo ____.png\"; filename*=UTF-8''Recibo%20%22%C3%A7%22%25.png"
        );
    }
}
