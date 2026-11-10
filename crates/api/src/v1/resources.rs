//! The resources of `v1` and their types ([ADR 0087]). These types are the contract: they are not the web app's, and a change
//! to a screen must not change them. They are written by hand beside `docs/api/v1/openapi.json`, and tests hold the two
//! together.

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use conta_verde_application::{
    ApplicationError, PartyKind, PartyQuery, ProductQuery, SaleClaimRepository, SaleQuery,
};
use conta_verde_domain::{
    Address, AddressDraft, Category, CategoryId, CostCenterId, EntryId, LineDraft, Party,
    PartyDraft, PartyId, PaymentTerm, PaymentTermId, Product, ProductId, Sale, SaleDraft, SaleId,
    SaleStatus,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cursor::{decode_cursor, decode_number_cursor, encode_cursor, encode_number_cursor};
use crate::error::ApiError;
use crate::json::{ApiJson, ApiQuery};
use crate::list_params;
use crate::v1::Principal;
use crate::{Ports, SharedState};

#[derive(Serialize)]
pub(crate) struct Page<T> {
    items: Vec<T>,
    next_cursor: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct List<T> {
    items: Vec<T>,
}

fn id<T>(raw: &str, not_found: ApplicationError, make: fn(Uuid) -> T) -> Result<T, ApiError> {
    Uuid::parse_str(raw).map(make).map_err(|_| not_found.into())
}

// ---- parties ----

#[derive(Serialize)]
pub(crate) struct V1Address {
    postal_code: String,
    street: String,
    number: Option<String>,
    complement: Option<String>,
    district: Option<String>,
    city: String,
    state: String,
}

impl From<&Address> for V1Address {
    fn from(address: &Address) -> Self {
        Self {
            postal_code: address.postal_code().to_owned(),
            street: address.street().to_owned(),
            number: address.number().map(str::to_owned),
            complement: address.complement().map(str::to_owned),
            district: address.district().map(str::to_owned),
            city: address.city().to_owned(),
            state: address.state().to_owned(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct V1Party {
    id: Uuid,
    name: String,
    document: Option<String>,
    email: Option<String>,
    is_customer: bool,
    is_supplier: bool,
    is_carrier: bool,
    is_seller: bool,
    address: Option<V1Address>,
    tags: Vec<String>,
}

impl From<&Party> for V1Party {
    fn from(party: &Party) -> Self {
        Self {
            id: party.id().as_uuid(),
            name: party.name().to_owned(),
            document: party
                .document()
                .map(|document| document.as_str().to_owned()),
            email: party.email().map(|email| email.as_str().to_owned()),
            is_customer: party.is_customer(),
            is_supplier: party.is_supplier(),
            is_carrier: party.is_carrier(),
            is_seller: party.is_seller(),
            address: party.address().map(V1Address::from),
            tags: party
                .tags()
                .iter()
                .map(|tag| tag.as_str().to_owned())
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V1AddressRequest {
    postal_code: String,
    street: String,
    number: Option<String>,
    complement: Option<String>,
    district: Option<String>,
    city: String,
    state: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V1PartyRequest {
    name: String,
    document: Option<String>,
    email: Option<String>,
    is_customer: bool,
    is_supplier: bool,
    address: Option<V1AddressRequest>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PartyListQuery {
    limit: Option<usize>,
    after: Option<String>,
    q: Option<String>,
    kind: Option<String>,
    sort: Option<String>,
}

pub(crate) async fn list_parties<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    ApiQuery(query): ApiQuery<PartyListQuery>,
) -> Result<axum::Json<Page<V1Party>>, ApiError> {
    let mut filter = PartyQuery::default();
    if let Some(text) = list_params::search(query.q.as_deref())? {
        filter = filter.searching(text);
    }
    filter = match query.kind.as_deref() {
        None => filter,
        Some("customer") => filter.of_kind(PartyKind::Customer),
        Some("supplier") => filter.of_kind(PartyKind::Supplier),
        Some("carrier") => filter.of_kind(PartyKind::Carrier),
        Some("seller") => filter.of_kind(PartyKind::Seller),
        Some(_) => {
            return Err(ApiError::InvalidQuery(
                "invalid kind: expected customer, supplier, carrier or seller",
            ));
        }
    };
    let filter = filter.sorted(list_params::sort(query.sort.as_deref())?);
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .parties
        .list(who.company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(axum::Json(Page {
        items: page.items.iter().map(V1Party::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn get_party<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    Path(party_id): Path<String>,
) -> Result<axum::Json<V1Party>, ApiError> {
    let party_id = id(
        &party_id,
        ApplicationError::PartyNotFound,
        PartyId::from_uuid,
    )?;
    let party = state.parties.get(who.company_id, party_id).await?;
    Ok(axum::Json(V1Party::from(&party)))
}

pub(crate) async fn register_party<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    ApiJson(request): ApiJson<V1PartyRequest>,
) -> Result<(StatusCode, axum::Json<V1Party>), ApiError> {
    let draft = PartyDraft {
        name: request.name,
        document: request.document,
        email: request.email,
        is_customer: request.is_customer,
        is_supplier: request.is_supplier,
        is_carrier: false,
        is_seller: false,
        address: request.address.map(|address| AddressDraft {
            postal_code: address.postal_code,
            street: address.street,
            number: address.number,
            complement: address.complement,
            district: address.district,
            city: address.city,
            state: address.state,
        }),
        contacts: Vec::new(),
        tags: Vec::new(),
    };
    let party = state
        .parties
        .register(who.company_id, &who.actor, draft)
        .await?;
    Ok((StatusCode::CREATED, axum::Json(V1Party::from(&party))))
}

// ---- products ----

#[derive(Serialize)]
pub(crate) struct V1Product {
    id: Uuid,
    /// `product` or `service`.
    kind: &'static str,
    sku: Option<String>,
    name: String,
    unit: String,
    /// A decimal string, or `null` when the product has no price.
    sale_price: Option<String>,
    active: bool,
}

impl From<&Product> for V1Product {
    fn from(product: &Product) -> Self {
        Self {
            id: product.id().as_uuid(),
            kind: product.kind().as_str(),
            sku: product.sku().map(str::to_owned),
            name: product.name().to_owned(),
            unit: product.unit().to_owned(),
            sale_price: product.sale_price().map(|price| price.to_string()),
            active: product.is_active(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProductListQuery {
    limit: Option<usize>,
    after: Option<String>,
    q: Option<String>,
    kind: Option<String>,
    active: Option<String>,
    sort: Option<String>,
}

pub(crate) async fn list_products<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    ApiQuery(query): ApiQuery<ProductListQuery>,
) -> Result<axum::Json<Page<V1Product>>, ApiError> {
    let mut filter = ProductQuery::default();
    if let Some(text) = list_params::search(query.q.as_deref())? {
        filter = filter.searching(text);
    }
    if let Some(kind) = query.kind.as_deref() {
        filter = filter.of_kind(conta_verde_domain::ProductKind::parse(kind).ok_or(
            ApiError::InvalidQuery("invalid kind: expected product or service"),
        )?);
    }
    if let Some(active) = list_params::active(query.active.as_deref())? {
        filter = filter.with_active(active);
    }
    let filter = filter.sorted(list_params::sort(query.sort.as_deref())?);
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .products
        .list(who.company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(axum::Json(Page {
        items: page.items.iter().map(V1Product::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

pub(crate) async fn get_product<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    Path(product_id): Path<String>,
) -> Result<axum::Json<V1Product>, ApiError> {
    let product_id = id(
        &product_id,
        ApplicationError::ProductNotFound,
        ProductId::from_uuid,
    )?;
    let product = state.products.get(who.company_id, product_id).await?;
    Ok(axum::Json(V1Product::from(&product)))
}

#[derive(Serialize)]
pub(crate) struct V1Stock {
    product_id: Uuid,
    /// A decimal string: what the books hold now.
    quantity: String,
    /// The minimum the company wants, or `null`.
    minimum_stock: Option<String>,
}

pub(crate) async fn product_stock<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    Path(product_id): Path<String>,
) -> Result<axum::Json<V1Stock>, ApiError> {
    let product_id = id(
        &product_id,
        ApplicationError::ProductNotFound,
        ProductId::from_uuid,
    )?;
    let product = state.products.get(who.company_id, product_id).await?;
    let balance = state.stock.balance_of(who.company_id, product_id).await?;
    Ok(axum::Json(V1Stock {
        product_id: product_id.as_uuid(),
        quantity: balance.quantity().normalize().to_string(),
        minimum_stock: product.minimum_stock().map(|minimum| minimum.to_string()),
    }))
}

// ---- what a sale needs to name: categories and payment terms ----

#[derive(Serialize)]
pub(crate) struct V1Category {
    id: Uuid,
    name: String,
    parent_id: Option<Uuid>,
    active: bool,
}

impl From<&Category> for V1Category {
    fn from(category: &Category) -> Self {
        Self {
            id: category.id().as_uuid(),
            name: category.name().to_owned(),
            parent_id: category.parent_id().map(|parent| parent.as_uuid()),
            active: category.is_active(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CategoryListQuery {
    active: Option<String>,
}

pub(crate) async fn list_categories<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    ApiQuery(query): ApiQuery<CategoryListQuery>,
) -> Result<axum::Json<List<V1Category>>, ApiError> {
    let active = list_params::active(query.active.as_deref())?;
    let categories = state.categories.list(who.company_id, active).await?;
    Ok(axum::Json(List {
        items: categories.iter().map(V1Category::from).collect(),
    }))
}

#[derive(Serialize)]
pub(crate) struct V1Installment {
    days: u32,
    /// A decimal string, the share of the total.
    percent: String,
}

#[derive(Serialize)]
pub(crate) struct V1PaymentTerm {
    id: Uuid,
    name: String,
    installments: Vec<V1Installment>,
    active: bool,
}

impl From<&PaymentTerm> for V1PaymentTerm {
    fn from(term: &PaymentTerm) -> Self {
        Self {
            id: term.id().as_uuid(),
            name: term.name().to_owned(),
            installments: term
                .installments()
                .iter()
                .map(|installment| V1Installment {
                    days: installment.days(),
                    percent: installment.percent().to_string(),
                })
                .collect(),
            active: term.is_active(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PaymentTermListQuery {
    limit: Option<usize>,
    after: Option<String>,
    active: Option<String>,
    sort: Option<String>,
}

pub(crate) async fn list_payment_terms<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    ApiQuery(query): ApiQuery<PaymentTermListQuery>,
) -> Result<axum::Json<Page<V1PaymentTerm>>, ApiError> {
    let mut filter = conta_verde_application::PaymentTermQuery::default();
    if let Some(active) = list_params::active(query.active.as_deref())? {
        filter = filter.with_active(active);
    }
    let filter = filter.sorted(list_params::sort(query.sort.as_deref())?);
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .payment_terms
        .list(who.company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(axum::Json(Page {
        items: page.items.iter().map(V1PaymentTerm::from).collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}

// ---- sales ----

#[derive(Serialize)]
pub(crate) struct V1Line {
    product_id: Uuid,
    description: String,
    /// A decimal string with at most 3 decimals.
    quantity: String,
    /// A decimal string with at most 6 decimals.
    unit_price: String,
    /// A decimal string: an amount of money, not a percentage.
    discount: String,
    /// What the line is worth, a decimal string.
    total: String,
}

#[derive(Serialize)]
pub(crate) struct V1Sale {
    id: Uuid,
    number: i64,
    /// The reference the order was made with, if it came with one.
    external_reference: Option<String>,
    /// `open`, `completed` or `cancelled`.
    status: &'static str,
    customer_id: Uuid,
    seller_id: Option<Uuid>,
    sold_on: String,
    payment_term_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    notes: Option<String>,
    lines: Vec<V1Line>,
    /// The sum of the lines, a decimal string.
    total: String,
    /// The receivable of a completed sale; `null` otherwise.
    receivable_id: Option<Uuid>,
}

impl V1Sale {
    fn new(sale: &Sale, external_reference: Option<String>, receivable_id: Option<Uuid>) -> Self {
        Self {
            id: sale.id().as_uuid(),
            number: sale.number(),
            external_reference,
            status: sale.status().as_str(),
            customer_id: sale.customer_id().as_uuid(),
            seller_id: sale.seller_id().map(|id| id.as_uuid()),
            sold_on: sale.sold_on().to_string(),
            payment_term_id: sale.payment_term_id().map(|id| id.as_uuid()),
            category_id: sale.category_id().as_uuid(),
            cost_center_id: sale.cost_center_id().map(|id| id.as_uuid()),
            notes: sale.notes().map(str::to_owned),
            lines: sale
                .lines()
                .iter()
                .map(|line| V1Line {
                    product_id: line.product_id().as_uuid(),
                    description: line.description().to_owned(),
                    quantity: line.quantity().to_string(),
                    unit_price: line.unit_price().to_string(),
                    discount: line.discount().to_string(),
                    total: line.total().to_string(),
                })
                .collect(),
            total: sale.total().to_string(),
            receivable_id,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V1LineRequest {
    product_id: Uuid,
    description: String,
    quantity: String,
    unit_price: String,
    discount: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V1SaleRequest {
    /// Names the order so that sending it twice makes one sale.
    external_reference: Option<String>,
    customer_id: Uuid,
    seller_id: Option<Uuid>,
    sold_on: String,
    payment_term_id: Option<Uuid>,
    category_id: Uuid,
    cost_center_id: Option<Uuid>,
    notes: Option<String>,
    lines: Vec<V1LineRequest>,
}

impl V1SaleRequest {
    fn draft(&self) -> SaleDraft {
        SaleDraft {
            customer_id: PartyId::from_uuid(self.customer_id),
            seller_id: self.seller_id.map(PartyId::from_uuid),
            sold_on: self.sold_on.clone(),
            payment_term_id: self.payment_term_id.map(PaymentTermId::from_uuid),
            category_id: CategoryId::from_uuid(self.category_id),
            cost_center_id: self.cost_center_id.map(CostCenterId::from_uuid),
            notes: self.notes.clone(),
            lines: self
                .lines
                .iter()
                .map(|line| LineDraft {
                    product_id: ProductId::from_uuid(line.product_id),
                    description: line.description.clone(),
                    quantity: line.quantity.clone(),
                    unit_price: line.unit_price.clone(),
                    discount: line.discount.clone().unwrap_or_else(|| "0".to_owned()),
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SaleListQuery {
    limit: Option<usize>,
    after: Option<String>,
    q: Option<String>,
    status: Option<String>,
    customer: Option<Uuid>,
    external_reference: Option<String>,
    sort: Option<String>,
}

async fn describe<X: Ports>(
    state: &SharedState<X>,
    sale: &Sale,
    with_receivable: bool,
) -> Result<V1Sale, ApiError> {
    let reference = state
        .sale_claims
        .reference_of(sale.company_id(), sale.id())
        .await
        .map_err(ApplicationError::from)?;
    let receivable = if with_receivable {
        state
            .sales
            .receivable(sale.company_id(), sale.id())
            .await?
            .map(|entry| entry.id().as_uuid())
    } else {
        None
    };
    Ok(V1Sale::new(sale, reference, receivable))
}

pub(crate) async fn list_sales<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    ApiQuery(query): ApiQuery<SaleListQuery>,
) -> Result<axum::Json<Page<V1Sale>>, ApiError> {
    // By the reference an order was made with: at most one sale, so there is no page to turn.
    if let Some(reference) = query.external_reference.as_deref() {
        let claim = state
            .sale_claims
            .find(who.company_id, reference.trim())
            .await
            .map_err(ApplicationError::from)?;
        let sale = match claim {
            Some(claim) => state.sales.get(who.company_id, claim.sale_id).await.ok(),
            None => None,
        };
        let items = match sale {
            Some(sale) => vec![describe(&state, &sale, false).await?],
            None => Vec::new(),
        };
        return Ok(axum::Json(Page {
            items,
            next_cursor: None,
        }));
    }
    let mut filter = SaleQuery::default();
    if let Some(text) = list_params::search(query.q.as_deref())? {
        filter = filter.searching(text);
    }
    if let Some(raw) = query.status.as_deref() {
        filter = filter.with_status(SaleStatus::parse(raw).ok_or(ApiError::InvalidQuery(
            "invalid status: expected open, completed or cancelled",
        ))?);
    }
    if let Some(customer) = query.customer {
        filter = filter.with_customer(PartyId::from_uuid(customer));
    }
    let filter = filter.sorted(list_params::number_sort(query.sort.as_deref())?);
    let after = query
        .after
        .as_deref()
        .map(decode_number_cursor)
        .transpose()?;
    let page = state
        .sales
        .list(who.company_id, &filter, after.as_ref(), query.limit)
        .await?;
    let mut items = Vec::with_capacity(page.items.len());
    for sale in &page.items {
        items.push(describe(&state, sale, false).await?);
    }
    Ok(axum::Json(Page {
        items,
        next_cursor: page.next.as_ref().map(encode_number_cursor),
    }))
}

/// Makes an order. With an `external_reference` the order is made once however many times it is sent: the answer is `201`
/// the first time and `200`, with the same sale, after; the same reference with another request is a `409`.
pub(crate) async fn register_sale<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    ApiJson(request): ApiJson<V1SaleRequest>,
) -> Result<(StatusCode, axum::Json<V1Sale>), ApiError> {
    let draft = request.draft();
    let (sale, made, reference) = match request.external_reference.as_deref() {
        None => {
            let sale = state
                .sales
                .register(who.company_id, &who.actor, draft)
                .await?;
            (sale, true, None)
        }
        Some(reference) => {
            let canonical =
                serde_json::to_string(&request).expect("a request is always serializable");
            let (sale, made) = state
                .sales
                .register_external(
                    &state.sale_claims,
                    who.company_id,
                    &who.actor,
                    reference,
                    &canonical,
                    draft,
                )
                .await?;
            (sale, made, Some(reference.trim().to_owned()))
        }
    };
    let status = if made {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, axum::Json(V1Sale::new(&sale, reference, None))))
}

fn sale_id(raw: &str) -> Result<SaleId, ApiError> {
    id(raw, ApplicationError::SaleNotFound, SaleId::from_uuid)
}

pub(crate) async fn get_sale<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    Path(sale_id_text): Path<String>,
) -> Result<axum::Json<V1Sale>, ApiError> {
    let sale = state
        .sales
        .get(who.company_id, sale_id(&sale_id_text)?)
        .await?;
    Ok(axum::Json(describe(&state, &sale, true).await?))
}

pub(crate) async fn complete_sale<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    Path(sale_id_text): Path<String>,
) -> Result<axum::Json<V1Sale>, ApiError> {
    let sale = state
        .sales
        .complete(who.company_id, &who.actor, sale_id(&sale_id_text)?)
        .await?;
    Ok(axum::Json(describe(&state, &sale, true).await?))
}

pub(crate) async fn cancel_sale<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    Path(sale_id_text): Path<String>,
) -> Result<axum::Json<V1Sale>, ApiError> {
    let sale = state
        .sales
        .cancel(who.company_id, &who.actor, sale_id(&sale_id_text)?)
        .await?;
    Ok(axum::Json(describe(&state, &sale, true).await?))
}

#[derive(Serialize)]
pub(crate) struct V1Charge {
    id: Uuid,
    /// The place of the installment of the receivable, from 1.
    installment: u16,
    /// `boleto`, `pix` or `card`.
    method: &'static str,
    /// `open`, `paid`, `cancelled` or `expired`.
    status: &'static str,
    amount: String,
    due_on: String,
    /// The boleto's line, to type in an app.
    digitable_line: Option<String>,
    /// The Pix copy-and-paste text.
    pix_payload: Option<String>,
    /// The provider's page: the boleto's paper or the card checkout.
    payment_url: Option<String>,
    paid_on: Option<String>,
    paid_amount: Option<String>,
}

/// The charges made to collect what a sale billed, and whether they were paid. A sale that is not completed has none.
pub(crate) async fn sale_charges<X: Ports>(
    State(state): State<SharedState<X>>,
    Extension(who): Extension<Principal>,
    Path(sale_id_text): Path<String>,
) -> Result<axum::Json<List<V1Charge>>, ApiError> {
    let sale_id = sale_id(&sale_id_text)?;
    let Some(receivable) = state.sales.receivable(who.company_id, sale_id).await? else {
        return Ok(axum::Json(List { items: Vec::new() }));
    };
    let charges = state
        .charges
        .list(
            who.company_id,
            EntryId::from_uuid(receivable.id().as_uuid()),
        )
        .await?;
    Ok(axum::Json(List {
        items: charges
            .iter()
            .map(|charge| V1Charge {
                id: charge.id().as_uuid(),
                installment: charge.installment(),
                method: charge.method().as_str(),
                status: charge.status().as_str(),
                amount: charge.amount().to_string(),
                due_on: charge.due_on().to_string(),
                digitable_line: charge.payment().digitable_line.clone(),
                pix_payload: charge.payment().pix_payload.clone(),
                payment_url: charge.payment().payment_url.clone(),
                paid_on: charge.receipt().map(|receipt| receipt.paid_on.to_string()),
                paid_amount: charge
                    .receipt()
                    .map(|receipt| receipt.paid_amount.to_string()),
            })
            .collect(),
    }))
}
