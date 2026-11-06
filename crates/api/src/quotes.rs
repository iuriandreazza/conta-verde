use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use conta_verde_application::{ApplicationError, QuoteQuery};
use conta_verde_domain::{
    Line, LineDraft, PartyId, PaymentTermId, ProductId, Quote, QuoteDraft, QuoteId, QuoteStatus,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::clock::today;
use crate::cursor::{decode_number_cursor, encode_number_cursor};
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::list_params;
use crate::quote_pdf::{PdfLine, PdfQuote, brl, quantity};
use crate::{Ports, SharedState};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LineRequest {
    product_id: Uuid,
    /// Usually the product's name; it is kept as written so that the quote reads the same later.
    description: String,
    /// A decimal string above 0 with at most three decimals.
    quantity: String,
    /// A decimal string with at most six decimals.
    unit_price: String,
    /// A decimal string in cents; none is 0.
    discount: Option<String>,
}

impl From<LineRequest> for LineDraft {
    fn from(line: LineRequest) -> Self {
        Self {
            product_id: ProductId::from_uuid(line.product_id),
            description: line.description,
            quantity: line.quantity,
            unit_price: line.unit_price,
            discount: line.discount.unwrap_or_else(|| "0".to_owned()),
        }
    }
}

/// The body of both writing and changing a quote: a change replaces every field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QuoteRequest {
    customer_id: Uuid,
    /// `YYYY-MM-DD`.
    issued_on: String,
    /// `YYYY-MM-DD`: the last day it can be approved.
    valid_until: String,
    payment_term_id: Option<Uuid>,
    notes: Option<String>,
    lines: Vec<LineRequest>,
}

impl From<QuoteRequest> for QuoteDraft {
    fn from(request: QuoteRequest) -> Self {
        Self {
            customer_id: PartyId::from_uuid(request.customer_id),
            issued_on: request.issued_on,
            valid_until: request.valid_until,
            payment_term_id: request.payment_term_id.map(PaymentTermId::from_uuid),
            notes: request.notes,
            lines: request.lines.into_iter().map(LineDraft::from).collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SendRequest {
    /// Where the link is e-mailed.
    to: String,
}

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// Text found in the number, the notes or a line's description.
    q: Option<String>,
    /// `draft`, `sent`, `approved`, `declined` or `converted`.
    status: Option<String>,
    customer: Option<Uuid>,
    /// `-number` (default) or `number`.
    sort: Option<String>,
}

impl ListQuery {
    fn quote_query(&self) -> Result<QuoteQuery, ApiError> {
        let mut query = QuoteQuery::default();
        if let Some(text) = list_params::search(self.q.as_deref())? {
            query = query.searching(text);
        }
        if let Some(raw) = self.status.as_deref() {
            query = query.with_status(QuoteStatus::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid status: expected draft, sent, approved, declined or converted",
            ))?);
        }
        if let Some(customer) = self.customer {
            query = query.with_customer(PartyId::from_uuid(customer));
        }
        Ok(query.sorted(list_params::number_sort(self.sort.as_deref())?))
    }
}

#[derive(Serialize)]
pub(crate) struct LineResponse {
    product_id: Uuid,
    description: String,
    quantity: String,
    unit_price: String,
    discount: String,
    /// What the line is worth: the rounded gross less the discount.
    total: String,
}

impl From<&Line> for LineResponse {
    fn from(line: &Line) -> Self {
        Self {
            product_id: line.product_id().as_uuid(),
            description: line.description().to_owned(),
            quantity: line.quantity().to_string(),
            unit_price: line.unit_price().to_string(),
            discount: line.discount().to_string(),
            total: line.total().to_string(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct QuoteResponse {
    id: Uuid,
    company_id: Uuid,
    number: i64,
    customer_id: Uuid,
    issued_on: String,
    valid_until: String,
    payment_term_id: Option<Uuid>,
    notes: Option<String>,
    status: &'static str,
    lines: Vec<LineResponse>,
    /// The sum of the lines.
    total: String,
}

impl From<&Quote> for QuoteResponse {
    fn from(quote: &Quote) -> Self {
        Self {
            id: quote.id().as_uuid(),
            company_id: quote.company_id().as_uuid(),
            number: quote.number(),
            customer_id: quote.customer_id().as_uuid(),
            issued_on: quote.issued_on().to_string(),
            valid_until: quote.valid_until().to_string(),
            payment_term_id: quote.payment_term_id().map(|id| id.as_uuid()),
            notes: quote.notes().map(str::to_owned),
            status: quote.status().as_str(),
            lines: quote.lines().iter().map(LineResponse::from).collect(),
            total: quote.total().to_string(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct QuotePageResponse {
    items: Vec<QuoteResponse>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct SentResponse {
    quote: QuoteResponse,
    /// The only time the link is shown: the store keeps a digest of its token.
    link: String,
    /// Whether the e-mail with the link was sent. When it was not, the link can be passed on by hand.
    emailed: bool,
}

// A string, parsed here: axum's own rejection for a malformed id is plain text, unlike the JSON
// errors of the rest of the API. The company id was already validated by `CompanyAccess`.
fn parse_id(raw: &str) -> Result<QuoteId, ApiError> {
    Uuid::parse_str(raw)
        .map(QuoteId::from_uuid)
        .map_err(|_| ApplicationError::QuoteNotFound.into())
}

pub(crate) async fn list_quotes<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<QuotePageResponse>, ApiError> {
    let filter = query.quote_query()?;
    let after = query
        .after
        .as_deref()
        .map(decode_number_cursor)
        .transpose()?;
    let page = state
        .quotes
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(QuotePageResponse {
        items: page.items.iter().map(QuoteResponse::from).collect(),
        next_cursor: page.next.as_ref().map(encode_number_cursor),
    }))
}

pub(crate) async fn register_quote<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<QuoteRequest>,
) -> Result<(StatusCode, Json<QuoteResponse>), ApiError> {
    let quote = state
        .quotes
        .register(company_id, &actor, request.into())
        .await?;
    Ok((StatusCode::CREATED, Json(QuoteResponse::from(&quote))))
}

pub(crate) async fn get_quote<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<QuoteResponse>, ApiError> {
    let quote = state.quotes.get(company_id, parse_id(&id)?).await?;
    Ok(Json(QuoteResponse::from(&quote)))
}

/// Replaces every editable field of a quote that is a draft or was sent. A sent one becomes a draft
/// again and its link stops working; nothing is written when nothing changes.
pub(crate) async fn update_quote<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<QuoteRequest>,
) -> Result<Json<QuoteResponse>, ApiError> {
    let quote = state
        .quotes
        .update(company_id, &actor, parse_id(&id)?, request.into())
        .await?;
    Ok(Json(QuoteResponse::from(&quote)))
}

/// What was recorded about a quote, oldest first.
pub(crate) async fn quote_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.quotes.history(company_id, parse_id(&id)?).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

/// Marks the quote sent and e-mails the customer the link to answer it through.
pub(crate) async fn send_quote<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<SendRequest>,
) -> Result<Json<SentResponse>, ApiError> {
    let company_name = state.auth.company_name(company_id).await?;
    let sent = state
        .quotes
        .send(
            company_id,
            &actor,
            parse_id(&id)?,
            &request.to,
            &company_name,
        )
        .await?;
    Ok(Json(SentResponse {
        quote: QuoteResponse::from(&sent.quote),
        link: sent.link,
        emailed: sent.emailed,
    }))
}

pub(crate) async fn quote_pdf<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    let quote = state.quotes.get(company_id, parse_id(&id)?).await?;
    pdf_response(&state, &quote).await
}

async fn pdf_response<X: Ports>(
    state: &SharedState<X>,
    quote: &Quote,
) -> Result<Response, ApiError> {
    let print = print_of(state, quote).await?;
    let bytes = crate::quote_pdf::render(&print);
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "inline; filename=\"orcamento-{}.pdf\"",
            quote.number()
        ))
        .expect("a number is a valid header value"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

/// The quote with the names that live in other registers, written the way it is printed.
async fn print_of<X: Ports>(state: &SharedState<X>, quote: &Quote) -> Result<PdfQuote, ApiError> {
    let company = quote.company_id();
    let customer = state.parties.get(company, quote.customer_id()).await?;
    let payment_term = match quote.payment_term_id() {
        Some(id) => Some(
            state
                .payment_terms
                .get(company, id)
                .await?
                .name()
                .to_owned(),
        ),
        None => None,
    };
    Ok(PdfQuote {
        company: state.auth.company_name(company).await?,
        customer: customer.name().to_owned(),
        number: quote.number(),
        issued_on: quote.issued_on().to_string(),
        valid_until: quote.valid_until().to_string(),
        payment_term,
        notes: quote.notes().map(str::to_owned),
        lines: quote
            .lines()
            .iter()
            .map(|line| PdfLine {
                description: line.description().to_owned(),
                quantity: quantity(&line.quantity().to_string()),
                unit_price: brl(&line.unit_price().to_string()),
                discount: brl(&line.discount().to_string()),
                total: brl(&line.total().to_string()),
            })
            .collect(),
        total: brl(&quote.total().to_string()),
    })
}

/// What the customer sees at the link: the quote and who offers it, nothing of the company's books.
#[derive(Serialize)]
pub(crate) struct PublicQuoteResponse {
    company_name: String,
    customer_name: String,
    number: i64,
    issued_on: String,
    valid_until: String,
    /// Whether the validity has run out today, which is when it can no longer be approved.
    expired: bool,
    payment_term: Option<String>,
    notes: Option<String>,
    status: &'static str,
    lines: Vec<LineResponse>,
    total: String,
}

async fn public_view<X: Ports>(
    state: &SharedState<X>,
    quote: &Quote,
) -> Result<Json<PublicQuoteResponse>, ApiError> {
    let print = print_of(state, quote).await?;
    Ok(Json(PublicQuoteResponse {
        company_name: print.company,
        customer_name: print.customer,
        number: quote.number(),
        issued_on: print.issued_on,
        valid_until: print.valid_until,
        expired: quote.is_expired_on(today()),
        payment_term: print.payment_term,
        notes: print.notes,
        status: quote.status().as_str(),
        lines: quote.lines().iter().map(LineResponse::from).collect(),
        total: quote.total().to_string(),
    }))
}

/// The quote a link leads to. It needs no session: the customer has none, and the token is the secret.
pub(crate) async fn public_quote<X: Ports>(
    State(state): State<SharedState<X>>,
    Path(token): Path<String>,
) -> Result<Json<PublicQuoteResponse>, ApiError> {
    let quote = state.quotes.by_link(&token).await?;
    public_view(&state, &quote).await
}

pub(crate) async fn public_quote_pdf<X: Ports>(
    State(state): State<SharedState<X>>,
    Path(token): Path<String>,
) -> Result<Response, ApiError> {
    let quote = state.quotes.by_link(&token).await?;
    pdf_response(&state, &quote).await
}

pub(crate) async fn approve_quote<X: Ports>(
    State(state): State<SharedState<X>>,
    Path(token): Path<String>,
) -> Result<Json<PublicQuoteResponse>, ApiError> {
    let quote = state.quotes.answer(&token, true, today()).await?;
    public_view(&state, &quote).await
}

pub(crate) async fn decline_quote<X: Ports>(
    State(state): State<SharedState<X>>,
    Path(token): Path<String>,
) -> Result<Json<PublicQuoteResponse>, ApiError> {
    let quote = state.quotes.answer(&token, false, today()).await?;
    public_view(&state, &quote).await
}
