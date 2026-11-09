use axum::Json;
use axum::extract::State;
use conta_verde_domain::{
    AddressDraft, FiscalEnvironment, FiscalSettings, FiscalSettingsDraft, NatureKind,
    OperationNature, OperationNatureDraft, TaxRegime, TaxRule, TaxRuleDraft,
};
use serde::{Deserialize, Serialize};

use crate::access::CompanyAccess;
use crate::error::ApiError;
use crate::history::HistoryEntryResponse;
use crate::json::ApiJson;
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
pub(crate) struct NatureResponse {
    code: String,
    name: String,
    kind: &'static str,
    cfop_in_state: Option<String>,
    cfop_interstate: Option<String>,
    active: bool,
    rules: Vec<TaxRuleResponse>,
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
pub(crate) struct NatureRequest {
    code: String,
    name: String,
    kind: String,
    cfop_in_state: Option<String>,
    cfop_interstate: Option<String>,
    active: bool,
    rules: Vec<TaxRuleRequest>,
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
