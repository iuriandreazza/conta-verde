use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use conta_verde_application::{
    ApplicationError, BuyerPresence, FiscalDocumentCursor, FiscalDocumentQuery, FiscalGateway,
    IssueNfce, IssueNfe, IssueNfse, PaymentForm, SpecialTaxRegime,
};
use conta_verde_domain::{
    AddressDraft, Amount, CalendarDate, FiscalDocument, FiscalDocumentId, FiscalEnvironment,
    FiscalEvent, FiscalEventId, FiscalEventSubject, FiscalFileKind, FiscalSettings,
    FiscalSettingsDraft, IssTaxation, NatureKind, OperationNature, OperationNatureDraft, SaleId,
    ServiceTerms, ServiceTermsDraft, TaxRegime, TaxRule, TaxRuleDraft,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::{ApiJson, ApiQuery};
use crate::parties::{AddressRequest, AddressResponse};
use crate::{Ports, SharedState};

#[derive(Serialize)]
pub(crate) struct TaxRuleResponse {
    regime: &'static str,
    ncm_prefix: Option<String>,
    icms_code: String,
    pis_code: String,
    cofins_code: String,
    icms_rate: Option<String>,
    pis_rate: Option<String>,
    cofins_rate: Option<String>,
}

impl From<&TaxRule> for TaxRuleResponse {
    fn from(rule: &TaxRule) -> Self {
        let rate = |rate: Option<conta_verde_domain::TaxRate>| rate.map(|rate| rate.to_string());
        Self {
            regime: rule.regime().key(),
            ncm_prefix: rule.ncm_prefix().map(str::to_owned),
            icms_code: rule.icms_code().to_owned(),
            pis_code: rule.pis_code().to_owned(),
            cofins_code: rule.cofins_code().to_owned(),
            icms_rate: rate(rule.icms_rate()),
            pis_rate: rate(rule.pis_rate()),
            cofins_rate: rate(rule.cofins_rate()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ServiceTermsResponse {
    national_tax_code: String,
    municipal_service_code: Option<String>,
    iss_taxation: &'static str,
    iss_rate: Option<String>,
}

impl From<&ServiceTerms> for ServiceTermsResponse {
    fn from(terms: &ServiceTerms) -> Self {
        Self {
            national_tax_code: terms.national_tax_code().to_owned(),
            municipal_service_code: terms.municipal_service_code().map(str::to_owned),
            iss_taxation: terms.iss_taxation().key(),
            iss_rate: terms.iss_rate().map(|rate| rate.to_string()),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct NatureResponse {
    code: String,
    name: String,
    kind: &'static str,
    cfop_in_state: Option<String>,
    cfop_interstate: Option<String>,
    active: bool,
    rules: Vec<TaxRuleResponse>,
    service: Option<ServiceTermsResponse>,
}

impl From<&OperationNature> for NatureResponse {
    fn from(nature: &OperationNature) -> Self {
        Self {
            code: nature.code().to_owned(),
            name: nature.name().to_owned(),
            kind: nature.kind().key(),
            cfop_in_state: nature.cfop_in_state().map(str::to_owned),
            cfop_interstate: nature.cfop_interstate().map(str::to_owned),
            active: nature.is_active(),
            rules: nature.rules().iter().map(TaxRuleResponse::from).collect(),
            service: nature.service().map(ServiceTermsResponse::from),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct FiscalSettingsResponse {
    legal_name: String,
    cnpj: String,
    state_registration: Option<String>,
    municipal_registration: Option<String>,
    address: AddressResponse,
    regime: &'static str,
    environment: &'static str,
    nfe_series: u16,
    nfce_series: u16,
    nfse_series: u16,
    /// The day the A1 certificate expires, as the person typed it; the certificate itself is at the gateway.
    certificate_valid_until: Option<String>,
    /// The IBGE code of the issuer's municipality, which an NFS-e names as where it is issued.
    city_ibge_code: Option<String>,
    natures: Vec<NatureResponse>,
}

impl From<&FiscalSettings> for FiscalSettingsResponse {
    fn from(settings: &FiscalSettings) -> Self {
        Self {
            legal_name: settings.legal_name().to_owned(),
            cnpj: settings.cnpj().to_owned(),
            state_registration: settings.state_registration().map(str::to_owned),
            municipal_registration: settings.municipal_registration().map(str::to_owned),
            address: AddressResponse::from(settings.address()),
            regime: settings.regime().key(),
            environment: settings.environment().key(),
            nfe_series: settings.nfe_series(),
            nfce_series: settings.nfce_series(),
            nfse_series: settings.nfse_series(),
            certificate_valid_until: settings
                .certificate_valid_until()
                .map(|day| day.to_string()),
            city_ibge_code: settings.city_ibge_code().map(str::to_owned),
            natures: settings
                .natures()
                .iter()
                .map(NatureResponse::from)
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaxRuleRequest {
    regime: String,
    ncm_prefix: Option<String>,
    icms_code: String,
    pis_code: String,
    cofins_code: String,
    icms_rate: Option<String>,
    pis_rate: Option<String>,
    cofins_rate: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiceTermsRequest {
    national_tax_code: String,
    municipal_service_code: Option<String>,
    iss_taxation: String,
    iss_rate: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NatureRequest {
    code: String,
    name: String,
    kind: String,
    cfop_in_state: Option<String>,
    cfop_interstate: Option<String>,
    active: bool,
    rules: Vec<TaxRuleRequest>,
    service: Option<ServiceTermsRequest>,
}

/// The whole settings, replaced by a `PUT`: everything is said, so an old client cannot drop a nature by leaving the
/// list out.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FiscalSettingsRequest {
    legal_name: String,
    cnpj: String,
    state_registration: Option<String>,
    municipal_registration: Option<String>,
    address: AddressRequest,
    regime: String,
    environment: String,
    nfe_series: u16,
    nfce_series: u16,
    nfse_series: u16,
    certificate_valid_until: Option<String>,
    city_ibge_code: Option<String>,
    natures: Vec<NatureRequest>,
}

const UNKNOWN_REGIME: ApiError =
    ApiError::InvalidField("regime must be simples_nacional or lucro_presumido");

impl TaxRuleRequest {
    fn into_draft(self) -> Result<TaxRuleDraft, ApiError> {
        Ok(TaxRuleDraft {
            regime: TaxRegime::parse(&self.regime).ok_or(UNKNOWN_REGIME)?,
            ncm_prefix: self.ncm_prefix,
            icms_code: self.icms_code,
            pis_code: self.pis_code,
            cofins_code: self.cofins_code,
            icms_rate: self.icms_rate,
            pis_rate: self.pis_rate,
            cofins_rate: self.cofins_rate,
        })
    }
}

impl ServiceTermsRequest {
    fn into_draft(self) -> Result<ServiceTermsDraft, ApiError> {
        Ok(ServiceTermsDraft {
            national_tax_code: self.national_tax_code,
            municipal_service_code: self.municipal_service_code,
            iss_taxation: IssTaxation::parse(&self.iss_taxation).ok_or(ApiError::InvalidField(
                "iss_taxation must be taxable, immune, export or not_incident",
            ))?,
            iss_rate: self.iss_rate,
        })
    }
}

impl NatureRequest {
    fn into_draft(self) -> Result<OperationNatureDraft, ApiError> {
        Ok(OperationNatureDraft {
            code: self.code,
            name: self.name,
            kind: NatureKind::parse(&self.kind)
                .ok_or(ApiError::InvalidField("kind must be product or service"))?,
            cfop_in_state: self.cfop_in_state,
            cfop_interstate: self.cfop_interstate,
            active: self.active,
            rules: self
                .rules
                .into_iter()
                .map(TaxRuleRequest::into_draft)
                .collect::<Result<_, _>>()?,
            service: self
                .service
                .map(ServiceTermsRequest::into_draft)
                .transpose()?,
        })
    }
}

impl FiscalSettingsRequest {
    fn into_draft(self) -> Result<FiscalSettingsDraft, ApiError> {
        Ok(FiscalSettingsDraft {
            legal_name: self.legal_name,
            cnpj: self.cnpj,
            state_registration: self.state_registration,
            municipal_registration: self.municipal_registration,
            address: AddressDraft::from(self.address),
            regime: TaxRegime::parse(&self.regime).ok_or(UNKNOWN_REGIME)?,
            environment: FiscalEnvironment::parse(&self.environment).ok_or(
                ApiError::InvalidField("environment must be homologation or production"),
            )?,
            nfe_series: self.nfe_series,
            nfce_series: self.nfce_series,
            nfse_series: self.nfse_series,
            certificate_valid_until: self.certificate_valid_until,
            city_ibge_code: self.city_ibge_code,
            natures: self
                .natures
                .into_iter()
                .map(NatureRequest::into_draft)
                .collect::<Result<_, _>>()?,
        })
    }
}

/// The company's fiscal settings, or `null` while it has not set them.
pub(crate) async fn get_settings<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Option<FiscalSettingsResponse>>, ApiError> {
    let settings = state.fiscal_settings.get(company_id).await?;
    Ok(Json(settings.as_ref().map(FiscalSettingsResponse::from)))
}

pub(crate) async fn replace_settings<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<FiscalSettingsRequest>,
) -> Result<Json<FiscalSettingsResponse>, ApiError> {
    let settings = state
        .fiscal_settings
        .replace(company_id, &actor, request.into_draft()?)
        .await?;
    Ok(Json(FiscalSettingsResponse::from(&settings)))
}

/// What was recorded about the company's fiscal settings, oldest first.
pub(crate) async fn settings_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state.fiscal_settings.history(company_id).await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

#[derive(Serialize)]
pub(crate) struct GatewayResponse {
    /// The gateway this installation issues through (`focus`, `fake`), or `null` when the operator configured none
    /// (`FISCAL_GATEWAY`): and then no document can be issued.
    gateway: Option<String>,
}

pub(crate) async fn get_gateway<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(_, _): CompanyAccess,
) -> Json<GatewayResponse> {
    let gateway = state.fiscal_documents.gateway();
    Json(GatewayResponse {
        gateway: gateway.is_enabled().then(|| gateway.key().to_owned()),
    })
}

#[derive(Serialize)]
pub(crate) struct RefusalResponse {
    code: String,
    message: String,
}

#[derive(Serialize)]
pub(crate) struct FiscalDocumentResponse {
    id: Uuid,
    kind: &'static str,
    status: &'static str,
    /// The cancellation that was asked and is waiting for the authority's answer, or was accepted ([ADR 0078]): a gateway
    /// that answers it later leaves the document authorized until then.
    cancellation: Option<FiscalEventResponse>,
    sale_id: Uuid,
    issuer_cnpj: String,
    issuer_name: String,
    environment: &'static str,
    recipient_name: String,
    recipient_document: Option<String>,
    nature_code: String,
    total: String,
    issued_on: String,
    /// An NF-e's access key, number and series; an NFS-e has none of them (`nfse` has its own).
    access_key: Option<String>,
    number: Option<u32>,
    series: Option<u16>,
    protocol: Option<String>,
    /// What the authority gave an authorized NFS-e.
    nfse: Option<NfseResponse>,
    /// The number the issuer gave the DPS of an NFS-e.
    dps: Option<DpsResponse>,
    refusal: Option<RefusalResponse>,
    note: Option<String>,
    submitted_by: String,
    /// The files that can be downloaded: `xml`, `pdf`, `cancellation_xml`.
    files: Vec<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct NfseResponse {
    number: u64,
    key: Option<String>,
    verification_code: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct DpsResponse {
    series: u16,
    number: u64,
}

fn document_response(
    document: &FiscalDocument,
    files: &[conta_verde_domain::FiscalFile],
    cancellation: Option<&FiscalEvent>,
) -> FiscalDocumentResponse {
    let conta_verde_domain::FiscalOrigin::Sale(sale) = document.origin();
    let authorization = document.authorization();
    FiscalDocumentResponse {
        id: document.id().as_uuid(),
        kind: document.kind().key(),
        status: document.status().key(),
        cancellation: cancellation.map(FiscalEventResponse::from),
        sale_id: sale.as_uuid(),
        issuer_cnpj: document.issuer_cnpj().to_owned(),
        issuer_name: document.issuer_name().to_owned(),
        environment: document.environment().key(),
        recipient_name: document.recipient_name().to_owned(),
        recipient_document: document.recipient_document().map(str::to_owned),
        nature_code: document.nature_code().to_owned(),
        total: document.total().as_decimal().to_string(),
        issued_on: document.issued_on().to_string(),
        access_key: authorization.map(|auth| auth.access_key().as_str().to_owned()),
        number: authorization.map(|auth| auth.number()),
        series: authorization.map(|auth| auth.series()),
        protocol: authorization
            .and_then(|auth| auth.protocol())
            .or_else(|| {
                document
                    .nfse_authorization()
                    .and_then(|nfse| nfse.protocol())
            })
            .map(str::to_owned),
        nfse: document.nfse_authorization().map(|nfse| NfseResponse {
            number: nfse.number(),
            key: nfse.key().map(str::to_owned),
            verification_code: nfse.verification_code().map(str::to_owned),
        }),
        dps: document.dps().map(|dps| DpsResponse {
            series: dps.series(),
            number: dps.number(),
        }),
        refusal: document.refusal().map(|refusal| RefusalResponse {
            code: refusal.code().to_owned(),
            message: refusal.message().to_owned(),
        }),
        note: document.note().map(str::to_owned),
        submitted_by: document.submitted_by_name().to_owned(),
        files: files.iter().map(|file| file.kind().key()).collect(),
    }
}

async fn respond<X: Ports>(
    state: &SharedState<X>,
    document: &FiscalDocument,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let files = state
        .fiscal_documents
        .files(document.company_id(), document.id())
        .await?;
    let cancellation = state
        .fiscal_documents
        .open_cancellation(document.company_id(), document.id())
        .await?;
    Ok(Json(document_response(
        document,
        &files,
        cancellation.as_ref(),
    )))
}

fn parse_document(raw: &str) -> Result<FiscalDocumentId, ApiError> {
    Uuid::parse_str(raw)
        .map(FiscalDocumentId::from_uuid)
        .map_err(|_| ApplicationError::FiscalDocumentNotFound.into())
}

/// What is chosen to issue: every field is a tax fact the product does not know, so none has a default ([ADR 0076]).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueRequest {
    sale_id: Uuid,
    nature_code: String,
    origin: u8,
    presence: String,
    payment: String,
    final_consumer: bool,
}

impl IssueRequest {
    fn into_request(self) -> Result<IssueNfe, ApiError> {
        let rule = |text: &str| ApplicationError::FiscalRule(text.to_owned());
        Ok(IssueNfe {
            sale_id: SaleId::from_uuid(self.sale_id),
            nature_code: self.nature_code,
            origin: self.origin,
            presence: BuyerPresence::parse(&self.presence)
                .ok_or_else(|| rule("presence is not one of the buyer presences"))?,
            payment: PaymentForm::parse(&self.payment)
                .ok_or_else(|| rule("payment is not one of the payment forms"))?,
            final_consumer: self.final_consumer,
        })
    }
}

/// Issues an NF-e from a completed sale, or answers with the one the sale already has.
pub(crate) async fn issue_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<IssueRequest>,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let document = state
        .fiscal_documents
        .issue(
            company_id,
            &actor,
            request.into_request()?,
            crate::now_unix(),
        )
        .await?;
    respond(&state, &document).await
}

pub(crate) async fn get_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let document = state
        .fiscal_documents
        .get(company_id, parse_document(&id)?)
        .await?;
    respond(&state, &document).await
}

pub(crate) async fn document_history<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<HistoryEntryResponse>>, ApiError> {
    let history = state
        .fiscal_documents
        .history(company_id, parse_document(&id)?)
        .await?;
    Ok(Json(
        history.iter().map(HistoryEntryResponse::from).collect(),
    ))
}

/// Asks the gateway about a document that is still waiting. A write by the method, so the `fiscal` area's writers.
pub(crate) async fn refresh_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let document = state
        .fiscal_documents
        .refresh(company_id, parse_document(&id)?, crate::now_unix())
        .await?;
    respond(&state, &document).await
}

/// Sends a rejected document again under the same id, with the choices made again.
pub(crate) async fn resend_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<IssueRequest>,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let document = state
        .fiscal_documents
        .resend(
            company_id,
            &actor,
            parse_document(&id)?,
            request.into_request()?,
            crate::now_unix(),
        )
        .await?;
    respond(&state, &document).await
}

/// What is chosen to issue an NFS-e: what the sale does not say and the product does not know, so none has a default
/// ([ADR 0081]). The ISSQN terms are the nature's, written in the settings.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueServiceRequest {
    sale_id: Uuid,
    nature_code: String,
    /// The competence day, `YYYY-MM-DD`.
    competence: String,
    /// The IBGE code of the municipality where the service was rendered.
    service_city_code: String,
    /// `none`, `cooperative_act`, `estimate`, `municipal_microenterprise`, `notary_or_registrar`,
    /// `autonomous_professional`, `professionals_society` or `other`.
    special_regime: String,
}

impl IssueServiceRequest {
    fn into_request(self) -> Result<IssueNfse, ApiError> {
        let rule = |text: &str| ApplicationError::FiscalRule(text.to_owned());
        Ok(IssueNfse {
            sale_id: SaleId::from_uuid(self.sale_id),
            nature_code: self.nature_code,
            competence: CalendarDate::parse(&self.competence)
                .ok_or_else(|| rule("the competence is a day written YYYY-MM-DD"))?,
            service_city_code: self.service_city_code,
            special_regime: SpecialTaxRegime::parse(&self.special_regime)
                .ok_or_else(|| rule("special_regime is not one of the special tax regimes"))?,
        })
    }
}

/// Issues an NFS-e from a completed sale, or answers with the one the sale already has.
pub(crate) async fn issue_service_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<IssueServiceRequest>,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let document = state
        .fiscal_documents
        .issue_nfse(
            company_id,
            &actor,
            request.into_request()?,
            crate::now_unix(),
        )
        .await?;
    respond(&state, &document).await
}

/// Sends a rejected NFS-e again under the same id and the same DPS number, with the choices made again.
pub(crate) async fn resend_service_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<IssueServiceRequest>,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let document = state
        .fiscal_documents
        .resend_nfse(
            company_id,
            &actor,
            parse_document(&id)?,
            request.into_request()?,
            crate::now_unix(),
        )
        .await?;
    respond(&state, &document).await
}

/// What is paid with one form, as a decimal string.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PaymentRequest {
    form: String,
    amount: String,
}

/// What is chosen to issue an NFC-e at the counter: every field is a fiscal fact the product does not know, so none has a
/// default ([ADR 0082]); what is paid adds up to the total, and no change is given.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueConsumerRequest {
    sale_id: Uuid,
    nature_code: String,
    origin: u8,
    presence: String,
    /// Whether the consumer is named on the invoice (the sale's customer).
    identify_consumer: bool,
    payments: Vec<PaymentRequest>,
}

impl IssueConsumerRequest {
    fn into_request(self) -> Result<IssueNfce, ApiError> {
        let rule = |text: &str| ApplicationError::FiscalRule(text.to_owned());
        Ok(IssueNfce {
            sale_id: SaleId::from_uuid(self.sale_id),
            nature_code: self.nature_code,
            origin: self.origin,
            presence: BuyerPresence::parse(&self.presence)
                .ok_or_else(|| rule("presence is not one of the buyer presences"))?,
            identify_consumer: self.identify_consumer,
            payments: self
                .payments
                .into_iter()
                .map(|payment| {
                    Ok((
                        PaymentForm::parse(&payment.form).ok_or_else(|| {
                            rule("a payment form is not one of the payment forms")
                        })?,
                        Amount::parse(&payment.amount)
                            .ok_or_else(|| rule("an amount paid is not a valid amount"))?,
                    ))
                })
                .collect::<Result<_, ApplicationError>>()?,
        })
    }
}

/// Issues an NFC-e from a completed sale, or answers with the one the sale already has.
pub(crate) async fn issue_consumer_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<IssueConsumerRequest>,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let document = state
        .fiscal_documents
        .issue_nfce(
            company_id,
            &actor,
            request.into_request()?,
            crate::now_unix(),
        )
        .await?;
    respond(&state, &document).await
}

/// Sends a rejected NFC-e again under the same id, with the choices made again.
pub(crate) async fn resend_consumer_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<IssueConsumerRequest>,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let document = state
        .fiscal_documents
        .resend_nfce(
            company_id,
            &actor,
            parse_document(&id)?,
            request.into_request()?,
            crate::now_unix(),
        )
        .await?;
    respond(&state, &document).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CancelRequest {
    reason: String,
}

pub(crate) async fn cancel_document<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<CancelRequest>,
) -> Result<Json<FiscalDocumentResponse>, ApiError> {
    let document = state
        .fiscal_documents
        .cancel(company_id, &actor, parse_document(&id)?, &request.reason)
        .await?;
    respond(&state, &document).await
}

/// Always as a download of the type the file has, with no sniffing: a fiscal file is never shown as a page of the
/// app's own origin.
pub(crate) async fn download_document_file<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id, kind)): Path<(Uuid, String, String)>,
) -> Result<Response, ApiError> {
    let kind = FiscalFileKind::parse(&kind).ok_or(ApplicationError::FiscalDocumentNotFound)?;
    let id = parse_document(&id)?;
    let bytes = state.fiscal_documents.file(company_id, id, kind).await?;
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(kind.content_type()),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}-{}.{}\"",
            kind.key(),
            id.as_uuid(),
            if kind == FiscalFileKind::Pdf {
                "pdf"
            } else {
                "xml"
            }
        ))
        .expect("a file name made of a key and an id is a header value"),
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

#[derive(Serialize)]
pub(crate) struct FiscalEventResponse {
    id: Uuid,
    kind: &'static str,
    status: &'static str,
    /// The letter's text, or the reason of the invalidation.
    text: String,
    /// The letter's place among its document's letters.
    sequence: Option<u8>,
    series: Option<u16>,
    first: Option<u32>,
    last: Option<u32>,
    protocol: Option<String>,
    refusal: Option<RefusalResponse>,
    requested_by: String,
    /// Whether the authority's XML of the event can be downloaded.
    has_file: bool,
}

impl From<&FiscalEvent> for FiscalEventResponse {
    fn from(event: &FiscalEvent) -> Self {
        let (sequence, series, first, last) = match event.subject() {
            FiscalEventSubject::Correction { sequence, .. } => (Some(*sequence), None, None, None),
            FiscalEventSubject::Cancellation { .. } => (None, None, None, None),
            FiscalEventSubject::Invalidation {
                series,
                first,
                last,
                ..
            } => (None, Some(*series), Some(*first), Some(*last)),
        };
        Self {
            id: event.id().as_uuid(),
            kind: event.subject().key(),
            status: event.status().key(),
            text: event.subject().body().to_owned(),
            sequence,
            series,
            first,
            last,
            protocol: event.protocol().map(str::to_owned),
            refusal: event.refusal().map(|refusal| RefusalResponse {
                code: refusal.code().to_owned(),
                message: refusal.message().to_owned(),
            }),
            requested_by: event.requested_by_name().to_owned(),
            has_file: event.xml().is_some(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionRequest {
    text: String,
}

pub(crate) async fn send_correction<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
    ApiJson(request): ApiJson<CorrectionRequest>,
) -> Result<Json<FiscalEventResponse>, ApiError> {
    let event = state
        .fiscal_documents
        .correct(company_id, &actor, parse_document(&id)?, &request.text)
        .await?;
    Ok(Json(FiscalEventResponse::from(&event)))
}

pub(crate) async fn list_corrections<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<Vec<FiscalEventResponse>>, ApiError> {
    let letters = state
        .fiscal_documents
        .corrections(company_id, parse_document(&id)?)
        .await?;
    Ok(Json(
        letters.iter().map(FiscalEventResponse::from).collect(),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvalidationRequest {
    series: u16,
    first: u32,
    last: u32,
    reason: String,
}

pub(crate) async fn invalidate_numbers<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    ApiJson(request): ApiJson<InvalidationRequest>,
) -> Result<Json<FiscalEventResponse>, ApiError> {
    let event = state
        .fiscal_documents
        .invalidate(
            company_id,
            &actor,
            request.series,
            request.first,
            request.last,
            &request.reason,
        )
        .await?;
    Ok(Json(FiscalEventResponse::from(&event)))
}

/// The company's invalidations, newest first (the latest 100).
pub(crate) async fn list_invalidations<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
) -> Result<Json<Vec<FiscalEventResponse>>, ApiError> {
    let found = state
        .fiscal_documents
        .invalidations(company_id, 100)
        .await?;
    Ok(Json(found.iter().map(FiscalEventResponse::from).collect()))
}

/// The authority's XML of an event it accepted, as a download that is never shown as a page.
pub(crate) async fn download_event_file<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    let id = Uuid::parse_str(&id)
        .map(FiscalEventId::from_uuid)
        .map_err(|_| ApplicationError::FiscalDocumentNotFound)?;
    let bytes = state.fiscal_documents.event_file(company_id, id).await?;
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"event-{}.xml\"",
            id.as_uuid()
        ))
        .expect("a file name made of an id is a header value"),
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

/// Gives up on an event that was sent and never answered, after the person checked at the gateway that the authority did
/// not take it.
pub(crate) async fn abandon_event<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, actor): CompanyAccess,
    Path((_, id)): Path<(Uuid, String)>,
) -> Result<Json<FiscalEventResponse>, ApiError> {
    let id = Uuid::parse_str(&id)
        .map(FiscalEventId::from_uuid)
        .map_err(|_| ApplicationError::FiscalDocumentNotFound)?;
    let event = state
        .fiscal_documents
        .abandon_event(company_id, &actor, id)
        .await?;
    Ok(Json(FiscalEventResponse::from(&event)))
}

#[derive(Serialize)]
pub(crate) struct FiscalDocumentPageResponse {
    items: Vec<FiscalDocumentResponse>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct DocumentListQuery {
    limit: Option<usize>,
    after: Option<String>,
    /// `draft`, `processing`, `authorized`, `contingency`, `rejected`, `denied` or `cancelled`.
    status: Option<String>,
    /// The sale a document was made from.
    sale: Option<Uuid>,
}

// Opaque to clients: the id first, because it has a fixed shape.
fn encode_cursor(cursor: &FiscalDocumentCursor) -> String {
    format!("{}|{}", cursor.id.as_uuid(), cursor.issued_at_unix)
}

fn decode_cursor(raw: &str) -> Result<FiscalDocumentCursor, ApiError> {
    let (id, moment) = raw.split_once('|').ok_or(ApiError::InvalidCursor)?;
    Ok(FiscalDocumentCursor {
        id: FiscalDocumentId::from_uuid(Uuid::parse_str(id).map_err(|_| ApiError::InvalidCursor)?),
        issued_at_unix: moment.parse().map_err(|_| ApiError::InvalidCursor)?,
    })
}

/// The company's documents, newest first, a page at a time.
pub(crate) async fn list_documents<X: Ports>(
    State(state): State<SharedState<X>>,
    CompanyAccess(company_id, _): CompanyAccess,
    ApiQuery(query): ApiQuery<DocumentListQuery>,
) -> Result<Json<FiscalDocumentPageResponse>, ApiError> {
    let status = query
        .status
        .as_deref()
        .map(|raw| {
            conta_verde_domain::FiscalDocumentStatus::parse(raw).ok_or(ApiError::InvalidQuery(
                "invalid status: expected draft, processing, authorized, contingency, rejected, denied or cancelled",
            ))
        })
        .transpose()?;
    let filter = FiscalDocumentQuery {
        status,
        sale: query.sale.map(SaleId::from_uuid),
    };
    let after = query.after.as_deref().map(decode_cursor).transpose()?;
    let page = state
        .fiscal_documents
        .list(company_id, &filter, after.as_ref(), query.limit)
        .await?;
    Ok(Json(FiscalDocumentPageResponse {
        // The files are not listed here: the detail has them.
        items: page
            .items
            .iter()
            .map(|document| document_response(document, &[], None))
            .collect(),
        next_cursor: page.next.as_ref().map(encode_cursor),
    }))
}
