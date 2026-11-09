use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use conta_verde_application::{ApplicationError, ExportTable, RepositoryError};
use conta_verde_domain::CalendarDate;
use serde::Deserialize;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::export_files::{ExportFormat, package_zip};
use crate::json::ApiQuery;
use crate::{Ports, SharedState};

#[derive(Deserialize)]
pub(crate) struct ExportParams {
    /// `YYYY-MM-DD`: the first day. Required, because the server does not know the person's today.
    from: String,
    /// `YYYY-MM-DD`: the last day.
    to: String,
    /// `csv` (the default) or `xlsx`.
    format: Option<String>,
}

struct Span {
    from: CalendarDate,
    to: CalendarDate,
    format: ExportFormat,
}

impl ExportParams {
    fn read(&self) -> Result<Span, ApiError> {
        let date = |raw: &str, message: &'static str| {
            CalendarDate::parse(raw).ok_or(ApiError::InvalidQuery(message))
        };
        let format = match self.format.as_deref() {
            None => ExportFormat::Csv,
            Some(raw) => {
                ExportFormat::parse(raw).ok_or(ApiError::InvalidQuery("format is csv or xlsx"))?
            }
        };
        Ok(Span {
            from: date(&self.from, "from is not a date (YYYY-MM-DD)")?,
            to: date(&self.to, "to is not a date (YYYY-MM-DD)")?,
            format,
        })
    }
}

/// A file to save: never shown in the browser, never sniffed, never kept by a cache ([ADR 0071]).
fn download(bytes: Vec<u8>, content_type: &'static str, filename: &str) -> Response {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .expect("a name made of dates and letters is a header value"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn table_download(table: &ExportTable, span: &Span) -> Response {
    let name = format!(
        "{}-{}-a-{}.{}",
        table.name,
        span.from,
        span.to,
        span.format.extension()
    );
    download(
        span.format.file_of(table),
        span.format.content_type(),
        &name,
    )
}

pub(crate) async fn entries_file<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(params): ApiQuery<ExportParams>,
) -> Result<Response, ApiError> {
    let span = params.read()?;
    let table = state
        .exports
        .entries_table(company_id, span.from, span.to)
        .await?;
    Ok(table_download(&table, &span))
}

pub(crate) async fn settlements_file<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(params): ApiQuery<ExportParams>,
) -> Result<Response, ApiError> {
    let span = params.read()?;
    let table = state
        .exports
        .settlements_table(company_id, span.from, span.to)
        .await?;
    Ok(table_download(&table, &span))
}

pub(crate) async fn transfers_file<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(params): ApiQuery<ExportParams>,
) -> Result<Response, ApiError> {
    let span = params.read()?;
    let table = state
        .exports
        .transfers_table(company_id, span.from, span.to)
        .await?;
    Ok(table_download(&table, &span))
}

/// The three tables, the list of attachments and the attached files, in one zip.
pub(crate) async fn package_file<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(params): ApiQuery<ExportParams>,
) -> Result<Response, ApiError> {
    let span = params.read()?;
    let package = state
        .exports
        .package(company_id, span.from, span.to)
        .await?;
    // A package is limited far below what a zip without ZIP64 holds, so this is a storage-sized fault: logged, not shown.
    let bytes = package_zip(package, span.format).map_err(|_| {
        ApplicationError::from(RepositoryError::Storage(
            "a package does not fit in a zip".to_owned(),
        ))
    })?;
    Ok(download(
        bytes,
        "application/zip",
        &format!("package-{}-a-{}.zip", span.from, span.to),
    ))
}
