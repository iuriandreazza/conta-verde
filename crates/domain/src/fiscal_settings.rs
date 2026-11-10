use rust_decimal::Decimal;

use crate::name::{optional_line, required_text};
use crate::{Address, AddressDraft, Amount, CalendarDate, CompanyId, Document, DomainError};

/// How many operation natures a company keeps and how many tax rules each holds: bounds that keep the settings a
/// page a person can read, not limits of the law.
pub const MAX_NATURES: usize = 50;
pub const MAX_RULES_PER_NATURE: usize = 20;

const MAX_LEGAL_NAME_CHARS: usize = 150;
const MAX_MUNICIPAL_REGISTRATION_CHARS: usize = 20;
const MAX_NATURE_CODE_CHARS: usize = 30;
const MAX_NATURE_NAME_CHARS: usize = 60;
const MAX_MUNICIPAL_SERVICE_CODE_CHARS: usize = 20;
const IBGE_CODE_DIGITS: usize = 7;
const MIN_NCM_PREFIX_DIGITS: usize = 2;
const MAX_NCM_PREFIX_DIGITS: usize = 8;
const CFOP_DIGITS: usize = 4;
/// The series of a document a company issues: a number from 1 to 999 (verify with an accountant which ranges the
/// SEFAZ reserves, [ADR 0073]).
pub const MIN_SERIES: u16 = 1;
pub const MAX_SERIES: u16 = 999;

/// The tax regimes the product issues for ([ADR 0073]). MEI, Simples with excess of the sublimit and Lucro Real are
/// not here on purpose: what changes for them is not known to the authors, so they are refused instead of guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaxRegime {
    SimplesNacional,
    LucroPresumido,
}

impl TaxRegime {
    pub fn key(self) -> &'static str {
        match self {
            Self::SimplesNacional => "simples_nacional",
            Self::LucroPresumido => "lucro_presumido",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [Self::SimplesNacional, Self::LucroPresumido]
            .into_iter()
            .find(|regime| regime.key() == raw)
    }
}

/// Which of the gateway's two environments the company's documents go to. Only a document of the production
/// environment is a fiscal document ([ADR 0073]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiscalEnvironment {
    Homologation,
    Production,
}

impl FiscalEnvironment {
    pub fn key(self) -> &'static str {
        match self {
            Self::Homologation => "homologation",
            Self::Production => "production",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [Self::Homologation, Self::Production]
            .into_iter()
            .find(|environment| environment.key() == raw)
    }
}

/// Whether an operation sells goods (an NF-e or NFC-e, with a CFOP) or a service (an NFS-e).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatureKind {
    Product,
    Service,
}

impl NatureKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Service => "service",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [Self::Product, Self::Service]
            .into_iter()
            .find(|kind| kind.key() == raw)
    }
}

/// How the ISSQN is charged on a service, in the national standard's own terms (`tribISSQN`). The product keeps four of its
/// values and **does not decide which one fits a service**: the company and its accountant do (verify with an accountant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssTaxation {
    /// The ISSQN is due: taxable operation.
    Taxable,
    Immune,
    Export,
    NotIncident,
}

impl IssTaxation {
    pub fn key(self) -> &'static str {
        match self {
            Self::Taxable => "taxable",
            Self::Immune => "immune",
            Self::Export => "export",
            Self::NotIncident => "not_incident",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [Self::Taxable, Self::Immune, Self::Export, Self::NotIncident]
            .into_iter()
            .find(|taxation| taxation.key() == raw)
    }
}

/// What a service nature says about the ISSQN of the services issued under it, as it is asked for: unvalidated.
#[derive(Debug, Clone)]
pub struct ServiceTermsDraft {
    pub national_tax_code: String,
    pub municipal_service_code: Option<String>,
    pub iss_taxation: IssTaxation,
    pub iss_rate: Option<String>,
}

/// What a service nature says about the ISSQN: the **national tax code** of the service (six digits: item, sub-item and
/// detail of the national list), the municipality's own code for it (a gateway whose layout is municipal asks for it), how
/// the tax is charged and, when the company sets it, the rate. The codes and the rate are the company's and its accountant's:
/// only their shape is checked ([ADR 0080]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceTerms {
    national_tax_code: String,
    municipal_service_code: Option<String>,
    iss_taxation: IssTaxation,
    iss_rate: Option<TaxRate>,
}

impl ServiceTerms {
    fn new(draft: &ServiceTermsDraft) -> Result<Self, DomainError> {
        Ok(Self {
            national_tax_code: digits_of(&draft.national_tax_code, 6, "national_tax_code")?,
            municipal_service_code: municipal_service_code_of(
                draft.municipal_service_code.as_deref(),
            )?,
            iss_taxation: draft.iss_taxation,
            iss_rate: rate_of(draft.iss_rate.as_deref(), "iss_rate")?,
        })
    }

    pub fn national_tax_code(&self) -> &str {
        &self.national_tax_code
    }

    pub fn municipal_service_code(&self) -> Option<&str> {
        self.municipal_service_code.as_deref()
    }

    pub fn iss_taxation(&self) -> IssTaxation {
        self.iss_taxation
    }

    pub fn iss_rate(&self) -> Option<TaxRate> {
        self.iss_rate
    }

    fn text(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.national_tax_code,
            self.municipal_service_code.as_deref().unwrap_or(""),
            self.iss_taxation.key(),
            self.iss_rate
                .map(|rate| rate.to_string())
                .unwrap_or_default()
        )
    }
}

/// The municipality's own code for a service: free text of one line, without the characters the trail's text separates its
/// parts with, so what it records can always be read back.
fn municipal_service_code_of(raw: Option<&str>) -> Result<Option<String>, DomainError> {
    let invalid = DomainError::InvalidFiscalField("municipal_service_code");
    let code = optional_line(raw, MAX_MUNICIPAL_SERVICE_CODE_CHARS, invalid.clone())?;
    match code {
        Some(code) if code.contains([':', '|', ';']) => Err(invalid),
        code => Ok(code),
    }
}

/// A tax rate as a percentage: 0 to 100 with at most two decimals. Unlike a [`crate::Percent`] it may be 0, since
/// a zero rate is a rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaxRate(Decimal);

impl TaxRate {
    pub fn parse(raw: &str) -> Option<Self> {
        let value = Amount::parse(raw.trim())?.as_decimal();
        (value <= Decimal::ONE_HUNDRED && value.scale() <= 2).then_some(Self(value))
    }

    pub fn as_decimal(&self) -> Decimal {
        self.0
    }
}

impl std::fmt::Display for TaxRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A tax rule as it is asked for: unvalidated, the rates as decimal text.
#[derive(Debug, Clone)]
pub struct TaxRuleDraft {
    pub regime: TaxRegime,
    pub ncm_prefix: Option<String>,
    pub icms_code: String,
    pub pis_code: String,
    pub cofins_code: String,
    pub icms_rate: Option<String>,
    pub pis_rate: Option<String>,
    pub cofins_rate: Option<String>,
}

/// Which codes and rates a company puts on the items of one operation nature, for one regime and, optionally, the
/// products whose NCM begins with a prefix. The codes and rates are **the company's** and its accountant's: the
/// product knows no tax table, it checks only their shape ([ADR 0073]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxRule {
    regime: TaxRegime,
    ncm_prefix: Option<String>,
    icms_code: String,
    pis_code: String,
    cofins_code: String,
    icms_rate: Option<TaxRate>,
    pis_rate: Option<TaxRate>,
    cofins_rate: Option<TaxRate>,
}

impl TaxRule {
    fn new(draft: &TaxRuleDraft) -> Result<Self, DomainError> {
        let ncm_prefix = optional_line(
            draft.ncm_prefix.as_deref(),
            MAX_NCM_PREFIX_DIGITS,
            DomainError::InvalidFiscalField("ncm_prefix"),
        )?;
        if let Some(prefix) = &ncm_prefix
            && (prefix.len() < MIN_NCM_PREFIX_DIGITS || !all_digits(prefix))
        {
            return Err(DomainError::InvalidFiscalField("ncm_prefix"));
        }
        // The ICMS code is a CSOSN (three digits) for the Simples Nacional and a CST (two) for the regular regimes.
        let icms_digits = match draft.regime {
            TaxRegime::SimplesNacional => 3,
            TaxRegime::LucroPresumido => 2,
        };
        Ok(Self {
            regime: draft.regime,
            ncm_prefix,
            icms_code: digits_of(&draft.icms_code, icms_digits, "icms_code")?,
            pis_code: digits_of(&draft.pis_code, 2, "pis_code")?,
            cofins_code: digits_of(&draft.cofins_code, 2, "cofins_code")?,
            icms_rate: rate_of(draft.icms_rate.as_deref(), "icms_rate")?,
            pis_rate: rate_of(draft.pis_rate.as_deref(), "pis_rate")?,
            cofins_rate: rate_of(draft.cofins_rate.as_deref(), "cofins_rate")?,
        })
    }

    pub fn regime(&self) -> TaxRegime {
        self.regime
    }

    pub fn ncm_prefix(&self) -> Option<&str> {
        self.ncm_prefix.as_deref()
    }

    pub fn icms_code(&self) -> &str {
        &self.icms_code
    }

    pub fn pis_code(&self) -> &str {
        &self.pis_code
    }

    pub fn cofins_code(&self) -> &str {
        &self.cofins_code
    }

    pub fn icms_rate(&self) -> Option<TaxRate> {
        self.icms_rate
    }

    pub fn pis_rate(&self) -> Option<TaxRate> {
        self.pis_rate
    }

    pub fn cofins_rate(&self) -> Option<TaxRate> {
        self.cofins_rate
    }

    fn text(&self) -> String {
        let rate = |rate: Option<TaxRate>| rate.map(|rate| rate.to_string()).unwrap_or_default();
        format!(
            "{}:{}:{}:{}:{}:{}:{}:{}",
            self.regime.key(),
            self.ncm_prefix.as_deref().unwrap_or(""),
            self.icms_code,
            self.pis_code,
            self.cofins_code,
            rate(self.icms_rate),
            rate(self.pis_rate),
            rate(self.cofins_rate)
        )
    }
}

/// An operation nature as it is asked for: unvalidated.
#[derive(Debug, Clone)]
pub struct OperationNatureDraft {
    pub code: String,
    pub name: String,
    pub kind: NatureKind,
    pub cfop_in_state: Option<String>,
    pub cfop_interstate: Option<String>,
    pub active: bool,
    pub rules: Vec<TaxRuleDraft>,
    /// For a service: what it says about the ISSQN. Optional while the company has not set it; an NFS-e needs it.
    pub service: Option<ServiceTermsDraft>,
}

/// What a company calls an operation (a sale, a sale for delivery, a service) with the CFOP that goes on its goods
/// inside the state and to another state, and the tax rules for its items. The `code` is the nature's stable key,
/// which a fiscal document records; the name is for people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationNature {
    code: String,
    name: String,
    kind: NatureKind,
    cfop_in_state: Option<String>,
    cfop_interstate: Option<String>,
    active: bool,
    rules: Vec<TaxRule>,
    service: Option<ServiceTerms>,
}

impl OperationNature {
    fn new(draft: &OperationNatureDraft) -> Result<Self, DomainError> {
        let code = draft.code.trim().to_ascii_lowercase();
        if code.is_empty()
            || code.chars().count() > MAX_NATURE_CODE_CHARS
            || !code
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(DomainError::InvalidFiscalField("code"));
        }
        let name = required_line(&draft.name, MAX_NATURE_NAME_CHARS, "name")?;
        if draft.kind == NatureKind::Product && draft.service.is_some() {
            return Err(DomainError::ProductNatureHasNoServiceTerms);
        }
        let blank = |raw: &Option<String>| raw.as_deref().is_none_or(|raw| raw.trim().is_empty());
        let (cfop_in_state, cfop_interstate, rules, service) = match draft.kind {
            NatureKind::Product => (
                Some(cfop_of(
                    draft.cfop_in_state.as_deref(),
                    '5',
                    "cfop_in_state",
                )?),
                Some(cfop_of(
                    draft.cfop_interstate.as_deref(),
                    '6',
                    "cfop_interstate",
                )?),
                rules_of(&draft.rules)?,
                None,
            ),
            // A service has no CFOP and no tax rules; what it says about the ISSQN is its terms (ADR 0080).
            NatureKind::Service => {
                if !blank(&draft.cfop_in_state) || !blank(&draft.cfop_interstate) {
                    return Err(DomainError::ServiceNatureHasNoCfop);
                }
                if !draft.rules.is_empty() {
                    return Err(DomainError::ServiceNatureHasNoRules);
                }
                (
                    None,
                    None,
                    Vec::new(),
                    draft.service.as_ref().map(ServiceTerms::new).transpose()?,
                )
            }
        };
        Ok(Self {
            code,
            name,
            kind: draft.kind,
            cfop_in_state,
            cfop_interstate,
            active: draft.active,
            rules,
            service,
        })
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> NatureKind {
        self.kind
    }

    pub fn cfop_in_state(&self) -> Option<&str> {
        self.cfop_in_state.as_deref()
    }

    pub fn cfop_interstate(&self) -> Option<&str> {
        self.cfop_interstate.as_deref()
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn rules(&self) -> &[TaxRule] {
        &self.rules
    }

    /// What a service nature says about the ISSQN, when the company has set it.
    pub fn service(&self) -> Option<&ServiceTerms> {
        self.service.as_ref()
    }

    /// The rule for an item of this regime and NCM ([ADR 0074]): the one whose NCM prefix is the **longest** the NCM
    /// begins with, else the nature's general rule (no prefix); `None` when no rule covers it. A rule of another
    /// regime never applies.
    pub fn rule_for(&self, regime: TaxRegime, ncm: &str) -> Option<&TaxRule> {
        self.rules
            .iter()
            .filter(|rule| rule.regime() == regime)
            .filter(|rule| {
                rule.ncm_prefix()
                    .is_none_or(|prefix| ncm.starts_with(prefix))
            })
            .max_by_key(|rule| rule.ncm_prefix().map_or(0, str::len))
    }

    fn text(&self) -> String {
        let text = format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.code,
            self.name,
            self.kind.key(),
            self.cfop_in_state.as_deref().unwrap_or(""),
            self.cfop_interstate.as_deref().unwrap_or(""),
            self.active,
            self.rules
                .iter()
                .map(TaxRule::text)
                .collect::<Vec<_>>()
                .join(";")
        );
        // The ISSQN terms are an eighth part only when there are some, so what the trail recorded before stays the same.
        match &self.service {
            Some(service) => format!("{text}|{}", service.text()),
            None => text,
        }
    }
}

/// Unvalidated input for the fiscal settings of a company: what a form sends.
#[derive(Debug, Clone)]
pub struct FiscalSettingsDraft {
    pub legal_name: String,
    pub cnpj: String,
    pub state_registration: Option<String>,
    pub municipal_registration: Option<String>,
    pub address: AddressDraft,
    pub regime: TaxRegime,
    pub environment: FiscalEnvironment,
    pub nfe_series: u16,
    pub nfce_series: u16,
    pub nfse_series: u16,
    pub certificate_valid_until: Option<String>,
    /// The IBGE code (seven digits) of the municipality the issuer is in: what an NFS-e says about where it is issued.
    pub city_ibge_code: Option<String>,
    pub natures: Vec<OperationNatureDraft>,
}

/// How a company issues fiscal documents ([ADR 0073], [ADR 0074]): who the issuer is, the tax regime, the gateway's
/// environment, the series of each kind of document, the day its A1 certificate expires and the operation natures with
/// their tax rules. One per company, replaced as a whole.
///
/// The certificate itself is **never here**: it stays at the gateway, and only the day it expires is kept, as the
/// person typed it. The number of a document is the gateway's, so there is no counter either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalSettings {
    company_id: CompanyId,
    legal_name: String,
    cnpj: String,
    state_registration: Option<String>,
    municipal_registration: Option<String>,
    address: Address,
    regime: TaxRegime,
    environment: FiscalEnvironment,
    nfe_series: u16,
    nfce_series: u16,
    nfse_series: u16,
    certificate_valid_until: Option<CalendarDate>,
    city_ibge_code: Option<String>,
    natures: Vec<OperationNature>,
}

impl FiscalSettings {
    pub fn new(company_id: CompanyId, draft: FiscalSettingsDraft) -> Result<Self, DomainError> {
        let legal_name = required_line(&draft.legal_name, MAX_LEGAL_NAME_CHARS, "legal_name")?;
        let cnpj = match Document::parse(&draft.cnpj)? {
            Document::Cnpj(cnpj) => cnpj,
            Document::Cpf(_) => return Err(DomainError::InvalidFiscalField("cnpj")),
        };
        let certificate_valid_until = draft
            .certificate_valid_until
            .as_deref()
            .map(str::trim)
            .filter(|raw| !raw.is_empty())
            .map(|raw| {
                CalendarDate::parse(raw).ok_or(DomainError::InvalidDate("certificate_valid_until"))
            })
            .transpose()?;
        if draft.environment == FiscalEnvironment::Production && certificate_valid_until.is_none() {
            return Err(DomainError::ProductionNeedsCertificate);
        }
        for (field, series) in [
            ("nfe_series", draft.nfe_series),
            ("nfce_series", draft.nfce_series),
            ("nfse_series", draft.nfse_series),
        ] {
            if !(MIN_SERIES..=MAX_SERIES).contains(&series) {
                return Err(DomainError::InvalidFiscalField(field));
            }
        }
        Ok(Self {
            company_id,
            legal_name,
            cnpj,
            state_registration: state_registration_of(draft.state_registration.as_deref())?,
            municipal_registration: optional_line(
                draft.municipal_registration.as_deref(),
                MAX_MUNICIPAL_REGISTRATION_CHARS,
                DomainError::InvalidFiscalField("municipal_registration"),
            )?,
            address: Address::new(draft.address)?,
            regime: draft.regime,
            environment: draft.environment,
            nfe_series: draft.nfe_series,
            nfce_series: draft.nfce_series,
            nfse_series: draft.nfse_series,
            certificate_valid_until,
            city_ibge_code: draft
                .city_ibge_code
                .as_deref()
                .map(str::trim)
                .filter(|raw| !raw.is_empty())
                .map(|raw| digits_of(raw, IBGE_CODE_DIGITS, "city_ibge_code"))
                .transpose()?,
            natures: natures_of(&draft.natures)?,
        })
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn legal_name(&self) -> &str {
        &self.legal_name
    }

    /// Fourteen characters, without punctuation.
    pub fn cnpj(&self) -> &str {
        &self.cnpj
    }

    pub fn state_registration(&self) -> Option<&str> {
        self.state_registration.as_deref()
    }

    pub fn municipal_registration(&self) -> Option<&str> {
        self.municipal_registration.as_deref()
    }

    pub fn address(&self) -> &Address {
        &self.address
    }

    pub fn regime(&self) -> TaxRegime {
        self.regime
    }

    pub fn environment(&self) -> FiscalEnvironment {
        self.environment
    }

    pub fn nfe_series(&self) -> u16 {
        self.nfe_series
    }

    pub fn nfce_series(&self) -> u16 {
        self.nfce_series
    }

    pub fn nfse_series(&self) -> u16 {
        self.nfse_series
    }

    pub fn certificate_valid_until(&self) -> Option<CalendarDate> {
        self.certificate_valid_until
    }

    pub fn city_ibge_code(&self) -> Option<&str> {
        self.city_ibge_code.as_deref()
    }

    pub fn natures(&self) -> &[OperationNature] {
        &self.natures
    }

    /// The natures in one canonical text, one per row, which is what the audit trail records: the same natures
    /// always read the same, however they were written.
    pub fn natures_text(&self) -> String {
        self.natures
            .iter()
            .map(OperationNature::text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A name on one line: the audit trail writes the natures one per row, and the database refuses control characters.
fn required_line(raw: &str, max: usize, field: &'static str) -> Result<String, DomainError> {
    let error = DomainError::InvalidFiscalField(field);
    let text = required_text(raw, max, error.clone())?;
    if text.chars().any(char::is_control) {
        return Err(error);
    }
    Ok(text)
}

fn all_digits(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Exactly `len` digits.
fn digits_of(raw: &str, len: usize, field: &'static str) -> Result<String, DomainError> {
    let text = raw.trim();
    if text.len() == len && all_digits(text) {
        Ok(text.to_owned())
    } else {
        Err(DomainError::InvalidFiscalField(field))
    }
}

/// A CFOP: four digits, the first saying where the goods go (5 inside the state, 6 to another).
fn cfop_of(raw: Option<&str>, first: char, field: &'static str) -> Result<String, DomainError> {
    let cfop = digits_of(raw.unwrap_or(""), CFOP_DIGITS, field)?;
    if cfop.starts_with(first) {
        Ok(cfop)
    } else {
        Err(DomainError::InvalidFiscalField(field))
    }
}

fn rate_of(raw: Option<&str>, field: &'static str) -> Result<Option<TaxRate>, DomainError> {
    raw.map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(|raw| TaxRate::parse(raw).ok_or(DomainError::InvalidFiscalField(field)))
        .transpose()
}

/// A state registration (IE): digits, written without the punctuation states give it, or the word `ISENTO`.
/// Only the shape is checked: each state has its own check digits (verify with an accountant, [ADR 0073]).
fn state_registration_of(raw: Option<&str>) -> Result<Option<String>, DomainError> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    if raw.eq_ignore_ascii_case("isento") {
        return Ok(Some("ISENTO".to_owned()));
    }
    let digits: String = raw
        .chars()
        .filter(|c| !matches!(c, '.' | '-' | '/') && !c.is_whitespace())
        .collect();
    if (2..=14).contains(&digits.len()) && all_digits(&digits) {
        Ok(Some(digits))
    } else {
        Err(DomainError::InvalidFiscalField("state_registration"))
    }
}

/// The rules that span the rules of a nature, which no single-row constraint can state: at most
/// [`MAX_RULES_PER_NATURE`], and one per regime and NCM prefix.
fn rules_of(drafts: &[TaxRuleDraft]) -> Result<Vec<TaxRule>, DomainError> {
    if drafts.len() > MAX_RULES_PER_NATURE {
        return Err(DomainError::TooManyTaxRules);
    }
    let mut rules: Vec<TaxRule> = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let rule = TaxRule::new(draft)?;
        if rules
            .iter()
            .any(|known| known.regime == rule.regime && known.ncm_prefix == rule.ncm_prefix)
        {
            return Err(DomainError::DuplicateTaxRule);
        }
        rules.push(rule);
    }
    Ok(rules)
}

/// The rules across the natures: at most [`MAX_NATURES`], and a code and a name (ignoring case) used once.
fn natures_of(drafts: &[OperationNatureDraft]) -> Result<Vec<OperationNature>, DomainError> {
    if drafts.len() > MAX_NATURES {
        return Err(DomainError::TooManyNatures);
    }
    let mut natures: Vec<OperationNature> = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let nature = OperationNature::new(draft)?;
        if natures.iter().any(|known| {
            known.code == nature.code || known.name.to_lowercase() == nature.name.to_lowercase()
        }) {
            return Err(DomainError::DuplicateNature);
        }
        natures.push(nature);
    }
    Ok(natures)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address() -> AddressDraft {
        AddressDraft {
            postal_code: "01310-100".into(),
            street: "Avenida Paulista".into(),
            number: Some("1000".into()),
            complement: None,
            district: Some("Bela Vista".into()),
            city: "São Paulo".into(),
            state: "SP".into(),
        }
    }

    fn rule(regime: TaxRegime, ncm: Option<&str>, icms: &str) -> TaxRuleDraft {
        TaxRuleDraft {
            regime,
            ncm_prefix: ncm.map(Into::into),
            icms_code: icms.into(),
            pis_code: "07".into(),
            cofins_code: "07".into(),
            icms_rate: None,
            pis_rate: None,
            cofins_rate: None,
        }
    }

    fn sale() -> OperationNatureDraft {
        OperationNatureDraft {
            code: "sale".into(),
            name: "Venda de mercadoria".into(),
            kind: NatureKind::Product,
            cfop_in_state: Some("5102".into()),
            cfop_interstate: Some("6102".into()),
            active: true,
            rules: vec![rule(TaxRegime::SimplesNacional, None, "102")],
            service: None,
        }
    }

    fn draft() -> FiscalSettingsDraft {
        FiscalSettingsDraft {
            legal_name: "Verde Comércio Ltda".into(),
            cnpj: "11.222.333/0001-81".into(),
            state_registration: Some("110.042.490.114".into()),
            municipal_registration: None,
            address: address(),
            regime: TaxRegime::SimplesNacional,
            environment: FiscalEnvironment::Homologation,
            nfe_series: 1,
            nfce_series: 1,
            nfse_series: 1,
            certificate_valid_until: Some("2027-03-31".into()),
            city_ibge_code: Some("3550308".into()),
            natures: vec![sale()],
        }
    }

    fn settings(
        change: impl FnOnce(&mut FiscalSettingsDraft),
    ) -> Result<FiscalSettings, DomainError> {
        let mut draft = draft();
        change(&mut draft);
        FiscalSettings::new(CompanyId::generate(), draft)
    }

    fn field(error: DomainError) -> &'static str {
        match error {
            DomainError::InvalidFiscalField(field) => field,
            other => panic!("not a field error: {other:?}"),
        }
    }

    #[test]
    fn keeps_the_issuer_as_it_will_be_sent() {
        let stored = settings(|_| {}).unwrap();
        assert_eq!(stored.cnpj(), "11222333000181");
        assert_eq!(stored.state_registration(), Some("110042490114"));
        assert_eq!(stored.address().postal_code(), "01310100");
        assert_eq!(stored.regime(), TaxRegime::SimplesNacional);
        assert_eq!(
            stored.certificate_valid_until(),
            CalendarDate::parse("2027-03-31")
        );
        assert_eq!(stored.natures().len(), 1);
    }

    #[test]
    fn the_rule_for_an_item_is_the_one_with_the_longest_prefix_of_the_regime() {
        let mut nature = sale();
        nature.rules = vec![
            rule(TaxRegime::SimplesNacional, None, "102"),
            rule(TaxRegime::SimplesNacional, Some("2106"), "500"),
            rule(TaxRegime::SimplesNacional, Some("21"), "400"),
            rule(TaxRegime::LucroPresumido, Some("21069090"), "41"),
        ];
        let stored = settings(|d| d.natures = vec![nature]).unwrap();
        let nature = &stored.natures()[0];
        let icms = |regime, ncm| nature.rule_for(regime, ncm).map(TaxRule::icms_code);
        assert_eq!(icms(TaxRegime::SimplesNacional, "21069090"), Some("500"));
        assert_eq!(icms(TaxRegime::SimplesNacional, "21011100"), Some("400"));
        assert_eq!(icms(TaxRegime::SimplesNacional, "09011100"), Some("102"));
        assert_eq!(icms(TaxRegime::LucroPresumido, "21069090"), Some("41"));
        assert_eq!(icms(TaxRegime::LucroPresumido, "09011100"), None);
    }

    #[test]
    fn the_issuer_is_a_company_not_a_person() {
        let cpf = settings(|d| d.cnpj = "529.982.247-25".into()).unwrap_err();
        assert_eq!(field(cpf), "cnpj");
        assert!(settings(|d| d.cnpj = "11.222.333/0001-82".into()).is_err());
    }

    #[test]
    fn a_state_registration_is_digits_or_isento() {
        let ie = |raw: &str| settings(|d| d.state_registration = Some(raw.into()));
        assert_eq!(ie("isento").unwrap().state_registration(), Some("ISENTO"));
        assert_eq!(ie("  ").unwrap().state_registration(), None);
        assert_eq!(field(ie("1").unwrap_err()), "state_registration");
        assert_eq!(field(ie("12ab34").unwrap_err()), "state_registration");
        assert_eq!(
            field(ie("123456789012345").unwrap_err()),
            "state_registration"
        );
    }

    #[test]
    fn a_series_is_from_1_to_999_for_each_kind() {
        assert!(settings(|d| d.nfe_series = 999).is_ok());
        assert_eq!(
            field(settings(|d| d.nfe_series = 0).unwrap_err()),
            "nfe_series"
        );
        assert_eq!(
            field(settings(|d| d.nfce_series = 1000).unwrap_err()),
            "nfce_series"
        );
        assert_eq!(
            field(settings(|d| d.nfse_series = 0).unwrap_err()),
            "nfse_series"
        );
    }

    #[test]
    fn production_needs_the_day_the_certificate_expires() {
        let production = |until: Option<&str>| {
            settings(|d| {
                d.environment = FiscalEnvironment::Production;
                d.certificate_valid_until = until.map(Into::into);
            })
        };
        assert!(production(Some("2027-03-31")).is_ok());
        assert_eq!(
            production(None).unwrap_err(),
            DomainError::ProductionNeedsCertificate
        );
        assert_eq!(
            production(Some("")).unwrap_err(),
            DomainError::ProductionNeedsCertificate
        );
        // Homologation does not need it, and a day that is not a day is refused whatever the environment.
        assert!(settings(|d| d.certificate_valid_until = None).is_ok());
        assert_eq!(
            settings(|d| d.certificate_valid_until = Some("2027-02-30".into())).unwrap_err(),
            DomainError::InvalidDate("certificate_valid_until")
        );
    }

    #[test]
    fn names_are_one_line() {
        assert_eq!(
            field(settings(|d| d.legal_name = "Verde\nLtda".into()).unwrap_err()),
            "legal_name"
        );
        assert_eq!(
            field(settings(|d| d.natures[0].name = "Venda\tbalcão".into()).unwrap_err()),
            "name"
        );
        assert_eq!(
            field(settings(|d| d.municipal_registration = Some("12\n34".into())).unwrap_err()),
            "municipal_registration"
        );
    }

    #[test]
    fn the_address_is_whole() {
        assert!(settings(|d| d.address.city = " ".into()).is_err());
    }

    #[test]
    fn a_product_nature_carries_a_cfop_for_inside_and_outside_the_state() {
        let with = |in_state: Option<&str>, interstate: Option<&str>| {
            settings(|d| {
                d.natures[0].cfop_in_state = in_state.map(Into::into);
                d.natures[0].cfop_interstate = interstate.map(Into::into);
            })
        };
        assert!(with(Some("5102"), Some("6102")).is_ok());
        assert_eq!(
            field(with(None, Some("6102")).unwrap_err()),
            "cfop_in_state"
        );
        assert_eq!(
            field(with(Some("6102"), Some("6102")).unwrap_err()),
            "cfop_in_state"
        );
        assert_eq!(
            field(with(Some("5102"), Some("5102")).unwrap_err()),
            "cfop_interstate"
        );
        assert_eq!(
            field(with(Some("510"), Some("6102")).unwrap_err()),
            "cfop_in_state"
        );
        assert_eq!(
            field(with(Some("51a2"), Some("6102")).unwrap_err()),
            "cfop_in_state"
        );
    }

    #[test]
    fn a_service_nature_has_no_cfop_and_no_rules() {
        let service = |change: fn(&mut OperationNatureDraft)| {
            settings(|d| {
                d.natures = vec![OperationNatureDraft {
                    code: "service".into(),
                    name: "Prestação de serviço".into(),
                    kind: NatureKind::Service,
                    cfop_in_state: None,
                    cfop_interstate: None,
                    active: true,
                    rules: vec![],
                    service: None,
                }];
                change(&mut d.natures[0]);
            })
        };
        assert!(service(|_| {}).is_ok());
        assert_eq!(
            service(|n| n.cfop_in_state = Some("5102".into())).unwrap_err(),
            DomainError::ServiceNatureHasNoCfop
        );
        assert_eq!(
            service(|n| n.rules = vec![rule(TaxRegime::SimplesNacional, None, "102")]).unwrap_err(),
            DomainError::ServiceNatureHasNoRules
        );
    }

    #[test]
    fn a_service_nature_may_carry_the_issqn_terms_and_a_product_may_not() {
        let terms = |national: &str, rate: Option<&str>| ServiceTermsDraft {
            national_tax_code: national.into(),
            municipal_service_code: Some(" 1.07 ".into()),
            iss_taxation: IssTaxation::Taxable,
            iss_rate: rate.map(Into::into),
        };
        let service = |terms: Option<ServiceTermsDraft>| {
            settings(|d| {
                d.natures = vec![OperationNatureDraft {
                    code: "service".into(),
                    name: "Prestação de serviço".into(),
                    kind: NatureKind::Service,
                    cfop_in_state: None,
                    cfop_interstate: None,
                    active: true,
                    rules: vec![],
                    service: terms,
                }];
            })
        };
        let stored = service(Some(terms("010701", Some("2.5")))).unwrap();
        let kept = stored.natures()[0].service().unwrap();
        assert_eq!(kept.national_tax_code(), "010701");
        assert_eq!(kept.municipal_service_code(), Some("1.07"));
        assert_eq!(kept.iss_taxation(), IssTaxation::Taxable);
        assert_eq!(kept.iss_rate().unwrap().to_string(), "2.5");
        assert!(service(None).unwrap().natures()[0].service().is_none());
        for bad in ["10701", "0107011", "01070a"] {
            assert_eq!(
                field(service(Some(terms(bad, None))).unwrap_err()),
                "national_tax_code"
            );
        }
        assert_eq!(
            field(service(Some(terms("010701", Some("101")))).unwrap_err()),
            "iss_rate"
        );
        let product = settings(|d| {
            d.natures[0].service = Some(terms("010701", None));
        });
        assert_eq!(
            product.unwrap_err(),
            DomainError::ProductNatureHasNoServiceTerms
        );
    }

    #[test]
    fn the_trail_text_of_a_nature_grows_a_part_only_for_a_service_with_terms() {
        let nature = |service: Option<ServiceTermsDraft>| {
            settings(|d| {
                d.natures = vec![OperationNatureDraft {
                    code: "service".into(),
                    name: "Serviço".into(),
                    kind: NatureKind::Service,
                    cfop_in_state: None,
                    cfop_interstate: None,
                    active: true,
                    rules: vec![],
                    service,
                }];
            })
            .unwrap()
            .natures_text()
        };
        let terms = |municipal: Option<&str>| ServiceTermsDraft {
            national_tax_code: "010701".into(),
            municipal_service_code: municipal.map(Into::into),
            iss_taxation: IssTaxation::Export,
            iss_rate: Some("2.5".into()),
        };
        assert_eq!(nature(None), "service|Serviço|service|||true|");
        assert_eq!(
            nature(Some(terms(Some("1.07")))),
            "service|Serviço|service|||true||010701:1.07:export:2.5"
        );
        // The separators of that text are never part of a municipal code, so it reads back whole.
        for bad in ["1:07", "1|07", "1;07"] {
            assert_eq!(
                field(
                    settings(|d| {
                        d.natures[0] = OperationNatureDraft {
                            code: "service".into(),
                            name: "Serviço".into(),
                            kind: NatureKind::Service,
                            cfop_in_state: None,
                            cfop_interstate: None,
                            active: true,
                            rules: vec![],
                            service: Some(terms(Some(bad))),
                        }
                    })
                    .unwrap_err()
                ),
                "municipal_service_code"
            );
        }
    }

    #[test]
    fn the_issuers_municipality_code_is_seven_digits_or_nothing() {
        assert_eq!(settings(|_| {}).unwrap().city_ibge_code(), Some("3550308"));
        assert_eq!(
            settings(|d| d.city_ibge_code = Some("  ".into()))
                .unwrap()
                .city_ibge_code(),
            None
        );
        assert_eq!(
            settings(|d| d.city_ibge_code = None)
                .unwrap()
                .city_ibge_code(),
            None
        );
        for bad in ["355030", "35503081", "35503a8"] {
            assert_eq!(
                field(settings(|d| d.city_ibge_code = Some(bad.into())).unwrap_err()),
                "city_ibge_code"
            );
        }
    }

    #[test]
    fn a_code_is_a_lower_case_key_and_a_code_and_a_name_are_used_once() {
        let natures = |codes: &[(&str, &str)]| {
            settings(|d| {
                d.natures = codes
                    .iter()
                    .map(|(code, name)| OperationNatureDraft {
                        code: (*code).into(),
                        name: (*name).into(),
                        ..sale()
                    })
                    .collect();
            })
        };
        assert_eq!(
            natures(&[(" Sale-1 ", "Venda")]).unwrap().natures()[0].code(),
            "sale-1"
        );
        assert_eq!(
            field(natures(&[("has space", "Venda")]).unwrap_err()),
            "code"
        );
        assert_eq!(field(natures(&[("", "Venda")]).unwrap_err()), "code");
        assert_eq!(field(natures(&[("a", " ")]).unwrap_err()), "name");
        assert_eq!(
            natures(&[("a", "Venda"), ("A", "Outra")]).unwrap_err(),
            DomainError::DuplicateNature
        );
        assert_eq!(
            natures(&[("a", "Venda"), ("b", "VENDA")]).unwrap_err(),
            DomainError::DuplicateNature
        );
    }

    #[test]
    fn the_number_of_natures_and_rules_is_bounded() {
        let many = |count: usize| {
            settings(|d| {
                d.natures = (0..count)
                    .map(|n| OperationNatureDraft {
                        code: format!("n{n}"),
                        name: format!("Nature {n}"),
                        ..sale()
                    })
                    .collect();
            })
        };
        assert!(many(MAX_NATURES).is_ok());
        assert_eq!(
            many(MAX_NATURES + 1).unwrap_err(),
            DomainError::TooManyNatures
        );

        let rules = |count: usize| {
            settings(|d| {
                d.natures[0].rules = (0..count)
                    .map(|n| {
                        rule(
                            TaxRegime::SimplesNacional,
                            Some(&format!("{:08}", n)),
                            "102",
                        )
                    })
                    .collect();
            })
        };
        assert!(rules(MAX_RULES_PER_NATURE).is_ok());
        assert_eq!(
            rules(MAX_RULES_PER_NATURE + 1).unwrap_err(),
            DomainError::TooManyTaxRules
        );
    }

    #[test]
    fn a_rule_is_checked_for_the_shape_of_its_codes_by_regime() {
        let with = |regime: TaxRegime, icms: &str, pis: &str| {
            settings(|d| {
                let mut one = rule(regime, None, icms);
                one.pis_code = pis.into();
                d.natures[0].rules = vec![one];
            })
        };
        // A CSOSN has three digits and a CST two.
        assert!(with(TaxRegime::SimplesNacional, "102", "07").is_ok());
        assert_eq!(
            field(with(TaxRegime::SimplesNacional, "10", "07").unwrap_err()),
            "icms_code"
        );
        assert!(with(TaxRegime::LucroPresumido, "00", "01").is_ok());
        assert_eq!(
            field(with(TaxRegime::LucroPresumido, "102", "01").unwrap_err()),
            "icms_code"
        );
        assert_eq!(
            field(with(TaxRegime::LucroPresumido, "00", "1").unwrap_err()),
            "pis_code"
        );
    }

    #[test]
    fn a_rate_is_from_0_to_100_with_two_decimals() {
        let rate = |raw: &str| {
            settings(|d| d.natures[0].rules[0].pis_rate = Some(raw.into())).map(|stored| {
                stored.natures()[0].rules()[0]
                    .pis_rate()
                    .unwrap()
                    .to_string()
            })
        };
        assert_eq!(rate("0.65").unwrap(), "0.65");
        assert_eq!(rate("0").unwrap(), "0");
        assert_eq!(rate("100").unwrap(), "100");
        for bad in ["100.01", "-1", "0.655", "3,00", "abc"] {
            assert_eq!(field(rate(bad).unwrap_err()), "pis_rate", "{bad}");
        }
        // Left out or blank is not set.
        assert_eq!(
            settings(|d| d.natures[0].rules[0].cofins_rate = Some(" ".into()))
                .unwrap()
                .natures()[0]
                .rules()[0]
                .cofins_rate(),
            None
        );
    }

    #[test]
    fn an_ncm_prefix_is_two_to_eight_digits_and_a_rule_is_one_per_regime_and_prefix() {
        let prefixed = |ncm: &str| {
            settings(|d| {
                d.natures[0].rules = vec![rule(TaxRegime::SimplesNacional, Some(ncm), "102")]
            })
        };
        assert!(prefixed("19").is_ok());
        assert!(prefixed("19059090").is_ok());
        for bad in ["1", "190590901", "19a"] {
            assert_eq!(field(prefixed(bad).unwrap_err()), "ncm_prefix", "{bad}");
        }
        let two = |a: (TaxRegime, Option<&str>), b: (TaxRegime, Option<&str>)| {
            settings(|d| {
                d.natures[0].rules = vec![rule(a.0, a.1, "102"), rule(b.0, b.1, "102")];
                d.natures[0].rules[1].icms_code = if b.0 == TaxRegime::LucroPresumido {
                    "00".into()
                } else {
                    "102".into()
                };
                d.natures[0].rules[0].icms_code = if a.0 == TaxRegime::LucroPresumido {
                    "00".into()
                } else {
                    "102".into()
                };
            })
        };
        let simples = TaxRegime::SimplesNacional;
        let presumido = TaxRegime::LucroPresumido;
        assert_eq!(
            two((simples, None), (simples, None)).unwrap_err(),
            DomainError::DuplicateTaxRule
        );
        assert_eq!(
            two((simples, Some("19")), (simples, Some("19"))).unwrap_err(),
            DomainError::DuplicateTaxRule
        );
        assert!(two((simples, None), (simples, Some("19"))).is_ok());
        assert!(two((simples, None), (presumido, None)).is_ok());
    }

    #[test]
    fn the_natures_read_the_same_whatever_they_were_written_like() {
        let one = settings(|_| {}).unwrap();
        assert_eq!(
            one.natures_text(),
            "sale|Venda de mercadoria|product|5102|6102|true|simples_nacional::102:07:07:::"
        );
        let other = settings(|d| d.natures[0].code = " SALE ".into()).unwrap();
        assert_eq!(one.natures_text(), other.natures_text());
    }
}
