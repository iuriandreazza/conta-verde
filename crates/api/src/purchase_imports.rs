use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use conta_verde_application::{InvoiceEntry, InvoiceOutcome, InvoicePreview};
use conta_verde_domain::{CategoryId, CostCenterId, PartyId, PaymentTermId, ProductId};
use conta_verde_infrastructure::parse_nfe;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::csv_import::read_file;
use crate::error::ApiError;
use crate::json::ApiQuery;
use crate::purchase_orders::ReceiptResponse;
use crate::{Ports, SharedState};

#[derive(Serialize)]
pub(crate) struct IssuerResponse {
    /// The CNPJ or CPF, without punctuation.
    document: String,
    name: String,
}

#[derive(Serialize)]
pub(crate) struct InvoiceItemResponse {
    number: u16,
    /// The supplier's code for the product.
    code: String,
    ean: Option<String>,
    description: String,
    ncm: Option<String>,
    unit: String,
    quantity: String,
    unit_value: String,
    product_value: String,
    discount: String,
    /// What the item costs: its products less discount plus the charges on it, the invoice's remainder shared.
    cost: String,
    /// The taxes the invoice shows for the item: IPI and ICMS ST are part of the cost, ICMS is not.
    ipi: String,
    icms_st: String,
    icms: String,
    /// The product registered with this code as its SKU, if one is.
    product_id: Option<Uuid>,
}

#[derive(Serialize)]
pub(crate) struct InstallmentResponse {
    due_date: String,
    amount: String,
}

#[derive(Serialize)]
pub(crate) struct EnteredResponse {
    purchase_id: Uuid,
    number: i64,
    status: &'static str,
}

#[derive(Serialize)]
pub(crate) struct InvoicePreviewResponse {
    access_key: String,
    number: String,
    series: String,
    issued_on: String,
    issuer: IssuerResponse,
    /// What the supplier is paid, in cents.
    total: String,
    /// The supplier registered with the issuer's registry number, if there is one.
    supplier_id: Option<Uuid>,
    items: Vec<InvoiceItemResponse>,
    /// What the invoice says is paid; empty when it does not say.
    installments: Vec<InstallmentResponse>,
    /// The purchase the invoice was already entered as.
    entered: Option<EnteredResponse>,
}

impl From<&InvoicePreview> for InvoicePreviewResponse {
    fn from(preview: &InvoicePreview) -> Self {
        let invoice = &preview.invoice;
        let costs = invoice.costs();
        Self {
            access_key: invoice.access_key().as_str().to_owned(),
            number: invoice.number().to_owned(),
            series: invoice.series().to_owned(),
            issued_on: invoice.issued_on().to_string(),
            issuer: IssuerResponse {
                document: invoice.issuer().as_str().to_owned(),
                name: invoice.issuer_name().to_owned(),
            },
            total: invoice.total().to_string(),
            supplier_id: preview.supplier.map(|id| id.as_uuid()),
            items: invoice
                .items()
                .iter()
                .zip(&costs)
                .zip(&preview.products)
                .map(|((item, cost), product)| InvoiceItemResponse {
                    number: item.number(),
                    code: item.code().to_owned(),
                    ean: item.ean().map(str::to_owned),
                    description: item.description().to_owned(),
                    ncm: item.ncm().map(str::to_owned),
                    unit: item.unit().to_owned(),
                    quantity: item.quantity().to_string(),
                    unit_value: item.unit_value().to_owned(),
                    product_value: item.product_value().to_string(),
                    discount: item.discount().to_string(),
                    cost: cost.to_string(),
                    ipi: item.ipi().to_string(),
                    icms_st: item.icms_st().to_string(),
                    icms: item.icms().to_string(),
                    product_id: product.map(|id| id.as_uuid()),
                })
                .collect(),
            installments: invoice
                .installments()
                .iter()
                .map(|(due, amount)| InstallmentResponse {
                    due_date: due.to_string(),
                    amount: amount.to_string(),
                })
                .collect(),
            entered: preview.entered.as_ref().map(|order| EnteredResponse {
                purchase_id: order.id().as_uuid(),
                number: order.number(),
                status: order.status().as_str(),
            }),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct EntryQuery {
    /// The category of the payables.
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    /// For the payable when the invoice gives no installments.
    payment_term_id: Option<Uuid>,
    /// `YYYY-MM-DD`: the day the goods arrived; the day of the invoice when left out.
    received_on: Option<String>,
    /// The supplier, when it is not the one the issuer's registry number finds.
    supplier_id: Option<Uuid>,
    /// The product each item becomes, in the order of the items, separated by commas; an empty place leaves the item
    /// to the product its supplier code finds.
    products: Option<String>,
}

impl EntryQuery {
    fn entry(&self) -> Result<InvoiceEntry, ApiError> {
        let products = match self.products.as_deref() {
            None | Some("") => Vec::new(),
            Some(list) => list
                .split(',')
                .map(|raw| {
                    let raw = raw.trim();
                    if raw.is_empty() {
                        Ok(None)
                    } else {
                        Uuid::parse_str(raw)
                            .map(|id| Some(ProductId::from_uuid(id)))
                            .map_err(|_| ApiError::InvalidQuery("invalid products: expected ids"))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(InvoiceEntry {
            category_id: CategoryId::from_uuid(self.category_id),
            cost_center_id: self.cost_center_id.map(CostCenterId::from_uuid),
            payment_term_id: self.payment_term_id.map(PaymentTermId::from_uuid),
            received_on: self.received_on.clone(),
            supplier_id: self.supplier_id.map(PartyId::from_uuid),
            products,
        })
    }
}

#[derive(Serialize)]
pub(crate) struct EntryResponse {
    purchase_id: Uuid,
    number: i64,
    status: &'static str,
    receipt: ReceiptResponse,
    /// The invoice had been entered before: nothing was made twice.
    already_entered: bool,
}

impl From<&InvoiceOutcome> for EntryResponse {
    fn from(outcome: &InvoiceOutcome) -> Self {
        Self {
            purchase_id: outcome.order.id().as_uuid(),
            number: outcome.order.number(),
            status: outcome.order.status().as_str(),
            receipt: ReceiptResponse::from(&outcome.receipt),
            already_entered: outcome.already_entered,
        }
    }
}

/// Reads the NF-e of the request body (its XML) and says what it is and how it meets the registers, without writing
/// anything.
pub(crate) async fn preview_invoice<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<InvoicePreviewResponse>, ApiError> {
    let bytes = read_file(&headers, body).await?;
    let draft = parse_nfe(&bytes).map_err(|error| ApiError::InvalidFile(error.to_string()))?;
    let preview = state.purchases.preview_invoice(company_id, draft).await?;
    Ok(Json((&preview).into()))
}

/// Enters the NF-e of the request body as a purchase received whole: payables and stock entries. The same invoice
/// twice changes nothing.
pub(crate) async fn enter_invoice<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiQuery(query): ApiQuery<EntryQuery>,
    headers: HeaderMap,
    body: Body,
) -> Result<(StatusCode, Json<EntryResponse>), ApiError> {
    let entry = query.entry()?;
    let bytes = read_file(&headers, body).await?;
    let draft = parse_nfe(&bytes).map_err(|error| ApiError::InvalidFile(error.to_string()))?;
    let outcome = state
        .purchases
        .enter_invoice(company_id, &actor, draft, entry)
        .await?;
    let status = if outcome.already_entered {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json((&outcome).into())))
}
