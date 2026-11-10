use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use conta_verde_application::{
    AcceptedEvent, AuthorizedDocument, AuthorizedNfse, Destination, DocumentOutcome, EventOutcome,
    EventQuery, FiscalGateway, GatewayError, NfeRequest, NfseRequest, PaymentForm, Refusal,
    Submission,
};
use conta_verde_domain::{AccessKey, Decimal, FiscalEnvironment, FiscalSettings, TaxRegime};
use reqwest::{Method, StatusCode};
use serde_json::{Map, Value, json};

use crate::brasil_api::client_without_redirects;
use crate::json_numbers::Numbers;

const DEFAULT_URL: &str = "https://api.nfse.io";
const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
/// What a reference of the gateway's own looks like when the document stores it: told apart from our id, which is a UUID.
const NAMED: &str = "nfeio:";
/// The text our reference is written into on the document (its complementary information), so that a document the
/// gateway made for it can be found again without knowing the gateway's own id.
const MARKER: &str = "cv-";
/// What the person reads when an NFS-e is asked of this gateway.
const NFSE_NOT_BUILT: &str = "an NFS-e through NFE.io: its published reference does not say how to find a service invoice by our reference, so none is submitted (use another gateway)";

/// Where the IBGE code of a municipality comes from. The NF-e needs the recipient's, and the registers keep only the name
/// and the state: asking is the adapter's business, and a name that finds no code, or finds two, is refused.
pub trait CityCodes: Send + Sync {
    fn code<'a>(
        &'a self,
        state: &'a str,
        city: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, GatewayError>> + Send + 'a>>;
}

/// The municipalities of a state with their IBGE codes, as a map from the name without accents or case: for tests and for
/// whoever knows the codes already.
#[derive(Debug, Clone, Default)]
pub struct StaticCityCodes(HashMap<(String, String), String>);

impl StaticCityCodes {
    pub fn with(mut self, state: &str, city: &str, code: &str) -> Self {
        self.0
            .insert((state.to_owned(), fold(city)), code.to_owned());
        self
    }
}

impl CityCodes for StaticCityCodes {
    fn code<'a>(
        &'a self,
        state: &'a str,
        city: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, GatewayError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.0.get(&(state.to_owned(), fold(city))).cloned()) })
    }
}

/// The codes of a state's municipalities, by the name without case or accents.
type Municipalities = HashMap<String, Vec<String>>;

/// The IBGE codes from BrasilAPI's public list of the municipalities of a state, read once per state.
#[derive(Clone)]
pub struct BrasilApiCityCodes {
    client: reqwest::Client,
    base_url: String,
    cache: Arc<Mutex<HashMap<String, Municipalities>>>,
}

impl BrasilApiCityCodes {
    pub fn new(base_url: Option<&str>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: client_without_redirects(TIMEOUT)?,
            base_url: base_url
                .unwrap_or("https://brasilapi.com.br")
                .trim_end_matches('/')
                .to_owned(),
            cache: Arc::default(),
        })
    }

    async fn municipalities(&self, state: &str) -> Result<Municipalities, GatewayError> {
        if let Some(known) = self.cache.lock().expect("city cache poisoned").get(state) {
            return Ok(known.clone());
        }
        let response = self
            .client
            .get(format!("{}/api/ibge/municipios/v1/{state}", self.base_url))
            .send()
            .await
            .map_err(|error| {
                GatewayError::Unavailable(format!("city codes: {}", error.without_url()))
            })?;
        if !response.status().is_success() {
            return Err(GatewayError::Unavailable(format!(
                "city codes: {}",
                response.status()
            )));
        }
        let list: Value = response.json().await.map_err(|error| {
            GatewayError::Unavailable(format!("city codes: {}", error.without_url()))
        })?;
        let mut by_name = Municipalities::new();
        for entry in list.as_array().into_iter().flatten() {
            if let (Some(name), Some(code)) = (
                entry["nome"].as_str(),
                entry["codigo_ibge"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| entry["codigo_ibge"].as_u64().map(|code| code.to_string())),
            ) {
                by_name.entry(fold(name)).or_default().push(code);
            }
        }
        // An empty list is a source that had nothing: asked again next time, never remembered.
        if !by_name.is_empty() {
            self.cache
                .lock()
                .expect("city cache poisoned")
                .insert(state.to_owned(), by_name.clone());
        }
        Ok(by_name)
    }
}

impl CityCodes for BrasilApiCityCodes {
    fn code<'a>(
        &'a self,
        state: &'a str,
        city: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, GatewayError>> + Send + 'a>> {
        Box::pin(async move {
            let by_name = self.municipalities(state).await?;
            Ok(match by_name.get(&fold(city)).map(Vec::as_slice) {
                Some([code]) => Some(code.clone()),
                // Two municipalities with one name in a state do not exist, and none is not guessed at.
                _ => None,
            })
        })
    }
}

/// A name without case, accents or surrounding space: how municipalities are compared.
fn fold(name: &str) -> String {
    name.trim()
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

struct Answer {
    status: StatusCode,
    body: Value,
}

/// A fiscal gateway that issues through [NFE.io](https://nfe.io) ([ADR 0078], [ADR 0079]): the NF-e through the product
/// invoices API v2, whose documents are named by the gateway, and whose cancellation and correction letter are answered
/// later. The company's certificate is at the gateway; the API key, the company ids and the issuer's CNPJ come from the
/// environment and are never stored or logged.
///
/// **Everything about its API was read from its published OpenAPI file and none of it exercised**: which words of the
/// search find the document we made, where the files are served from, how a refusal or a correction letter shows in the
/// events, and what the answer to an issue carries. The sandbox test is for those. Until the operator says the search was
/// confirmed (`NFEIO_SEARCH_CONFIRMED`), **production issues are refused**: a lost answer followed by a second submit could
/// otherwise issue two documents.
#[derive(Clone)]
pub struct NfeIoGateway {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    company_homologation: Option<String>,
    company_production: Option<String>,
    issuer_cnpj: String,
    cities: Arc<dyn CityCodes>,
    file_hosts: Vec<String>,
    search_confirmed: bool,
    /// The gateway's address is on this machine (a stand-in): only then may a file be fetched from here.
    local: bool,
}

impl NfeIoGateway {
    /// `base_url` replaces the gateway's host (a stand-in in a test). A company id that is not given makes the calls of its
    /// environment unavailable.
    pub fn new(
        base_url: Option<&str>,
        api_key: &str,
        company_homologation: Option<&str>,
        company_production: Option<&str>,
        issuer_cnpj: &str,
        cities: Arc<dyn CityCodes>,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: client_without_redirects(TIMEOUT)?,
            base_url: base_url
                .unwrap_or(DEFAULT_URL)
                .trim_end_matches('/')
                .to_owned(),
            api_key: api_key.trim().to_owned(),
            company_homologation: company_homologation.map(|id| id.trim().to_owned()),
            company_production: company_production.map(|id| id.trim().to_owned()),
            issuer_cnpj: issuer_cnpj.to_owned(),
            cities,
            file_hosts: vec!["nfe.io".to_owned(), "nfse.io".to_owned()],
            search_confirmed: false,
            local: base_url.is_some_and(|url| {
                url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost")
            }),
        })
    }

    /// Other hosts a file may be served from (a storage the gateway uses), besides its own.
    pub fn with_file_hosts(mut self, hosts: &[String]) -> Self {
        self.file_hosts.extend(
            hosts
                .iter()
                .map(|host| host.trim().trim_matches('.').to_lowercase())
                .filter(|host| !host.is_empty()),
        );
        self
    }

    /// The operator confirmed in the sandbox that the document made for our reference is found again by the search.
    pub fn with_search_confirmed(mut self, confirmed: bool) -> Self {
        self.search_confirmed = confirmed;
        self
    }

    fn target(&self, issuer: &FiscalSettings) -> Result<(String, &'static str), GatewayError> {
        if issuer.cnpj() != self.issuer_cnpj {
            return Err(GatewayError::WrongIssuer);
        }
        let (company, environment) = match issuer.environment() {
            FiscalEnvironment::Homologation => (&self.company_homologation, "Test"),
            FiscalEnvironment::Production => (&self.company_production, "Production"),
        };
        let company = company.clone().ok_or_else(|| {
            GatewayError::Unavailable(format!(
                "no NFE.io company is configured for the {} environment",
                issuer.environment().key()
            ))
        })?;
        if !company
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || company.is_empty()
        {
            return Err(GatewayError::Unavailable(
                "the NFE.io company id is not a plain id".to_owned(),
            ));
        }
        Ok((company, environment))
    }

    fn invoices(&self, company: &str) -> String {
        format!("{}/v2/companies/{company}/productinvoices", self.base_url)
    }

    async fn call(
        &self,
        method: Method,
        url: &str,
        query: &[(&str, &str)],
        body: Option<String>,
    ) -> Result<Answer, GatewayError> {
        let mut address = reqwest::Url::parse(url).map_err(|_| {
            GatewayError::Unavailable("the gateway's address is not valid".to_owned())
        })?;
        if !query.is_empty() {
            address.query_pairs_mut().extend_pairs(query);
        }
        let mut request = self
            .client
            .request(method, address)
            .header("authorization", &self.api_key);
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body);
        }
        let response = request.send().await.map_err(|error| {
            GatewayError::Unavailable(format!("request failed: {}", error.without_url()))
        })?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|error| {
            GatewayError::Unavailable(format!("the answer was cut: {}", error.without_url()))
        })?;
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            match serde_json::from_slice(&bytes) {
                Ok(body) => body,
                Err(_) if status.is_success() => {
                    return Err(GatewayError::Unavailable(
                        "a success whose answer is not JSON".to_owned(),
                    ));
                }
                Err(_) => Value::Null,
            }
        };
        Ok(Answer { status, body })
    }

    /// The name this adapter stores a document under, and the id inside it.
    fn named(id: &str) -> String {
        format!("{NAMED}{id}")
    }

    /// The gateway's id of the document for a reference: the name it was stored under, or, for our own id, what the
    /// search finds. `None` when the gateway has no document for it.
    async fn invoice_id(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<Option<String>, GatewayError> {
        if let Some(id) = reference.strip_prefix(NAMED) {
            return if plain(id) {
                Ok(Some(id.to_owned()))
            } else {
                Err(GatewayError::Unavailable(
                    "the gateway's reference is not a plain id".to_owned(),
                ))
            };
        }
        Ok(self
            .find(issuer, reference)
            .await?
            .and_then(|invoice| invoice["id"].as_str().map(str::to_owned)))
    }

    /// The document the gateway made for our reference, if any. **It must be able to tell**: a search that fails, or that
    /// cannot say whether there are more, is `Unavailable`, because the next step is to submit. The answer is checked
    /// against the marker here, not taken on the word of the search; a document that ended in an error does not count
    /// (the person sends it again), and of several the newest is the one.
    async fn find(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<Option<Value>, GatewayError> {
        let (company, environment) = self.target(issuer)?;
        let marker = format!("{MARKER}{reference}");
        let answer = self
            .call(
                Method::GET,
                &self.invoices(&company),
                &[
                    ("environment", environment),
                    ("limit", "20"),
                    ("q", &format!("additionalInformation.taxpayer:\"{marker}\"")),
                ],
                None,
            )
            .await?;
        if !answer.status.is_success() {
            return Err(GatewayError::Unavailable(format!(
                "the search for a document answered {}",
                answer.status
            )));
        }
        let invoices = answer.body["productInvoices"]
            .as_array()
            .ok_or_else(|| GatewayError::Unavailable("the search answered no list".to_owned()))?;
        let mut found: Vec<&Value> = invoices
            .iter()
            .filter(|invoice| {
                invoice["additionalInformation"]["taxpayer"]
                    .as_str()
                    .is_some_and(|text| text.contains(&marker))
                    // An invoice that ended in an error, or whose number was declared unused, is not the document:
                    // the person is sending it again.
                    && !matches!(invoice["status"].as_str(), Some("Error" | "Disabled"))
            })
            .collect();
        // A marker that is ours alone matches a handful of documents: more than a page is a search that cannot be trusted,
        // and the next step would be to submit.
        if answer.body["hasMore"].as_bool() == Some(true) {
            return Err(GatewayError::Unavailable(
                "the search has more answers than one page".to_owned(),
            ));
        }
        found.sort_by_key(|invoice| invoice["createdOn"].as_str().unwrap_or_default().to_owned());
        Ok(found.last().map(|invoice| (*invoice).clone()))
    }

    /// A file the gateway made: its address is asked for, then followed only over https and only to a host the gateway or
    /// the operator named, with no redirect.
    async fn file(
        &self,
        company: &str,
        id: &str,
        what: &str,
    ) -> Result<Option<Vec<u8>>, GatewayError> {
        let answer = self
            .call(
                Method::GET,
                &format!("{}/{id}/{what}", self.invoices(company)),
                &[],
                None,
            )
            .await?;
        if answer.status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !answer.status.is_success() {
            return Err(GatewayError::Unavailable(format!(
                "the {what} address answered {}",
                answer.status
            )));
        }
        let Some(uri) = answer.body["uri"].as_str() else {
            return Ok(None);
        };
        let parsed = reqwest::Url::parse(uri)
            .map_err(|_| GatewayError::Unavailable("a file address is not valid".to_owned()))?;
        let host = parsed.host_str().unwrap_or_default().to_lowercase();
        let allowed = parsed.scheme() == "https"
            && self
                .file_hosts
                .iter()
                .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")));
        // A local address is followed only when the gateway itself is local (a stand-in in a test): never from a
        // gateway's answer in production, where it would reach this server's own routes.
        let local =
            self.local && parsed.scheme() == "http" && (host == "127.0.0.1" || host == "localhost");
        if !allowed && !local {
            return Err(GatewayError::Unavailable(format!(
                "a file is served from a host the gateway did not name: {host}"
            )));
        }
        let response = self.client.get(parsed).send().await.map_err(|error| {
            GatewayError::Unavailable(format!("download failed: {}", error.without_url()))
        })?;
        if !response.status().is_success() {
            return Err(GatewayError::Unavailable(format!(
                "a file could not be downloaded: {}",
                response.status()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_FILE_BYTES as u64)
        {
            return Err(GatewayError::Unavailable("a file is too big".to_owned()));
        }
        let bytes = response.bytes().await.map_err(|error| {
            GatewayError::Unavailable(format!("download failed: {}", error.without_url()))
        })?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(GatewayError::Unavailable("a file is too big".to_owned()));
        }
        Ok(Some(bytes.to_vec()))
    }

    /// What the gateway's account of a document means, with its files fetched when it was authorized or cancelled.
    async fn outcome_of(
        &self,
        company: &str,
        invoice: &Value,
    ) -> Result<DocumentOutcome, GatewayError> {
        let refusal = || {
            Refusal::new(
                "recusado",
                invoice["authorization"]["message"]
                    .as_str()
                    .unwrap_or("the document was refused"),
            )
        };
        match invoice["status"].as_str() {
            Some("None" | "Created" | "Processing") | None => Ok(DocumentOutcome::Processing),
            Some("IssuedContingency") => Ok(DocumentOutcome::Contingency),
            Some("IssueDenied") => Ok(DocumentOutcome::Denied(refusal())),
            Some("Error") => Ok(DocumentOutcome::Rejected(refusal())),
            Some("Disabled") => Ok(DocumentOutcome::Rejected(Refusal::new(
                "inutilizada",
                "the number was declared unused",
            ))),
            Some(status @ ("Issued" | "Cancelled")) => {
                let id = invoice["id"].as_str().ok_or_else(|| {
                    GatewayError::Unavailable("the document came without its id".to_owned())
                })?;
                let document = self.authorized(company, id, invoice).await?;
                if status == "Issued" {
                    Ok(DocumentOutcome::Authorized(document))
                } else {
                    Ok(DocumentOutcome::Cancelled {
                        document,
                        // The authority's XML of the cancellation is under the events of the authority, whose shape was
                        // not verified: the cancellation is told without it (ADR 0079).
                        cancellation_xml: Vec::new(),
                    })
                }
            }
            Some(other) => Err(GatewayError::Unavailable(format!(
                "a status this adapter does not know: {other}"
            ))),
        }
    }

    async fn authorized(
        &self,
        company: &str,
        id: &str,
        invoice: &Value,
    ) -> Result<AuthorizedDocument, GatewayError> {
        let key = invoice["authorization"]["accessKey"]
            .as_str()
            .ok_or_else(|| {
                GatewayError::Unavailable("an authorized document without its access key".into())
            })?;
        let access_key = AccessKey::parse(key.trim_start_matches("NFe")).map_err(|_| {
            GatewayError::Unavailable("the access key of the document is not valid".to_owned())
        })?;
        let number = invoice["number"]
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .ok_or_else(|| GatewayError::Unavailable("a document without its number".into()))?;
        let series = invoice["serie"]
            .as_u64()
            .and_then(|series| u16::try_from(series).ok())
            .ok_or_else(|| GatewayError::Unavailable("a document without its series".into()))?;
        let xml = self.file(company, id, "xml").await?.ok_or_else(|| {
            GatewayError::Unavailable("an authorized document without its XML".to_owned())
        })?;
        // The printed form is a convenience: a failure to fetch it does not keep the authorization from being recorded.
        let pdf = self.file(company, id, "pdf").await.ok().flatten();
        Ok(AuthorizedDocument {
            access_key,
            number,
            series,
            // The authority's protocol is not in the answer the specification documents.
            protocol: None,
            xml,
            pdf,
        })
    }

    async fn invoice(&self, company: &str, id: &str) -> Result<Option<Value>, GatewayError> {
        let answer = self
            .call(
                Method::GET,
                &format!("{}/{id}", self.invoices(company)),
                &[],
                None,
            )
            .await?;
        match answer.status {
            StatusCode::NOT_FOUND => Ok(None),
            status if status.is_success() => Ok(Some(answer.body)),
            status => Err(GatewayError::Unavailable(format!(
                "the document query answered {status}"
            ))),
        }
    }

    /// An event that the gateway took: queued, to be learned later. A refusal of the request as a whole is an answer.
    fn queued_or_refused(answer: &Answer) -> Result<EventOutcome, GatewayError> {
        match answer.status {
            status if status.is_success() => Ok(EventOutcome::Queued),
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY => {
                Ok(EventOutcome::Refused(refusal_of(&answer.body)))
            }
            status => Err(GatewayError::Unavailable(format!(
                "the event request answered {status}"
            ))),
        }
    }

    async fn submit(&self, request: &NfeRequest) -> Result<Submission, GatewayError> {
        let (company, _) = self.target(&request.issuer)?;
        if request.issuer.regime() != TaxRegime::SimplesNacional {
            return Err(GatewayError::Unsupported(
                "an NF-e for Lucro Presumido, which needs the tax calculation",
            ));
        }
        if request.issuer.environment() == FiscalEnvironment::Production && !self.search_confirmed {
            return Err(GatewayError::Unsupported(
                "issuing in production before the search for a document by our reference was confirmed in the sandbox (NFEIO_SEARCH_CONFIRMED)",
            ));
        }
        // Never submit without knowing the gateway has nothing for this reference: a second submit is a second document.
        if let Some(invoice) = self.find(&request.issuer, &request.reference).await? {
            let id = invoice["id"].as_str().ok_or_else(|| {
                GatewayError::Unavailable("a found document without its id".to_owned())
            })?;
            return Ok(Submission {
                reference: Some(Self::named(id)),
                outcome: self.outcome_of(&company, &invoice).await?,
            });
        }
        let body = self.body(request).await?;
        let answer = self
            .call(Method::POST, &self.invoices(&company), &[], Some(body))
            .await?;
        match answer.status {
            status if status.is_success() => {
                // The id the gateway gave, or, when the answer does not carry one, what the search finds now.
                let id = match answer.body["id"].as_str().filter(|id| plain(id)) {
                    Some(id) => id.to_owned(),
                    None => self
                        .find(&request.issuer, &request.reference)
                        .await?
                        .and_then(|invoice| invoice["id"].as_str().map(str::to_owned))
                        .ok_or_else(|| {
                            GatewayError::Unavailable(
                                "the document was queued but the gateway did not say its id"
                                    .to_owned(),
                            )
                        })?,
                };
                Ok(Submission {
                    reference: Some(Self::named(&id)),
                    outcome: DocumentOutcome::Processing,
                })
            }
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => Ok(Submission::of(
                DocumentOutcome::Rejected(refusal_of(&answer.body)),
            )),
            status => Err(GatewayError::Unavailable(format!(
                "the submission answered {status}"
            ))),
        }
    }

    /// The body of an NF-e, in the fields the published specification gives, with every amount as the exact decimal it is.
    async fn body(&self, request: &NfeRequest) -> Result<String, GatewayError> {
        let recipient = &request.recipient;
        let address = recipient
            .address
            .as_ref()
            .ok_or(GatewayError::Unsupported("a recipient without an address"))?;
        let document = recipient
            .document
            .as_deref()
            .ok_or(GatewayError::Unsupported("a recipient without a document"))?;
        // The gateway's v2 takes the tax number as a JSON integer: an alphanumeric CNPJ needs its v3.
        let tax_number = document.parse::<u64>().map_err(|_| {
            GatewayError::Unsupported("a recipient whose document is not numeric (NFE.io v3)")
        })?;
        let city_code = self
            .cities
            .code(address.state(), address.city())
            .await?
            .ok_or(GatewayError::Unsupported(
                "a recipient whose municipality has no IBGE code the adapter can find",
            ))?;
        let district = address.district().ok_or(GatewayError::Unsupported(
            "a recipient whose address has no district",
        ))?;
        let numbers = Numbers::new();
        let decimal = |value: &Decimal| numbers.of(value.normalize());
        let money = |value: &conta_verde_domain::Amount| numbers.of(value.as_decimal());
        let mut items = Vec::with_capacity(request.items.len());
        for item in &request.items {
            let cfop: u32 = item
                .cfop
                .parse()
                .map_err(|_| GatewayError::Unsupported("a CFOP that is not a number"))?;
            items.push(json!({
                "code": item.code,
                "description": item.description,
                "ncm": item.ncm,
                "cfop": numbers.of(cfop),
                "unit": item.unit,
                "quantity": decimal(&item.quantity),
                "unitAmount": decimal(&item.unit_price),
                "totalAmount": money(&item.gross),
                "unitTax": item.unit,
                "quantityTax": decimal(&item.quantity),
                "taxUnitAmount": decimal(&item.unit_price),
                "discountAmount": money(&item.discount),
                "totalIndicator": true,
                "tax": {
                    "icms": { "origin": item.origin.to_string(), "csosn": item.icms_code },
                    "pis": { "cst": item.pis_code },
                    "cofins": { "cst": item.cofins_code },
                },
            }));
        }
        let mut buyer = Map::new();
        buyer.insert("name".into(), json!(recipient.name));
        buyer.insert("federalTaxNumber".into(), numbers.of(tax_number));
        buyer.insert(
            "type".into(),
            json!(if document.len() == 14 {
                "LegalEntity"
            } else {
                "NaturalPerson"
            }),
        );
        match &recipient.state_registration {
            Some(registration) => {
                buyer.insert("stateTaxNumberIndicator".into(), json!("TaxPayer"));
                buyer.insert("stateTaxNumber".into(), json!(registration));
            }
            None => {
                buyer.insert("stateTaxNumberIndicator".into(), json!("NonTaxPayer"));
            }
        }
        if let Some(email) = &recipient.email {
            buyer.insert("email".into(), json!(email));
        }
        buyer.insert(
            "address".into(),
            json!({
                "country": "BRA",
                "state": address.state(),
                "city": { "code": city_code, "name": address.city() },
                "district": district,
                "street": address.street(),
                "number": address.number().unwrap_or("S/N"),
                "postalCode": address.postal_code(),
                "additionalInformation": address.complement(),
            }),
        );
        let issued_on = utc_timestamp(request.issued_at_unix).ok_or_else(|| {
            GatewayError::Unavailable("the moment of issue is out of range".to_owned())
        })?;
        let body = json!({
            "operationNature": request.nature,
            "operationType": "Outgoing",
            "purposeType": "Normal",
            "issuedOn": issued_on,
            "destination": match request.destination {
                Destination::InState => "Internal_Operation",
                Destination::Interstate => "Interstate_Operation",
                Destination::Foreign => "International_Operation",
            },
            "consumerType": if request.final_consumer { "FinalConsumer" } else { "Normal" },
            "presenceType": presence_of(request.presence.key()),
            "buyer": Value::Object(buyer),
            "items": items,
            "payment": [{
                "paymentDetail": request.payments.iter().map(|(form, amount)| json!({
                    "method": method_of(*form),
                    "amount": money(amount),
                })).collect::<Vec<_>>(),
            }],
            // Our reference, on the document, where a search can find it again without the gateway's id.
            "additionalInformation": {
                "taxpayer": format!("Ref. {MARKER}{}", request.reference),
            },
        });
        Ok(numbers.render(&body))
    }
}

/// The name of an event of the gateway that is a correction letter: its own words, not any word that holds the letters
/// "cce" (`Accepted`).
fn is_correction_letter(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("correction") || name == "cce" || name.starts_with("cce_")
}

fn plain(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The code and text a refusal came with: `{"errors": [{"code", "message"}]}`.
fn refusal_of(body: &Value) -> Refusal {
    let first = body["errors"].as_array().and_then(|errors| errors.first());
    let code = first
        .and_then(|error| error["code"].as_i64())
        .map_or_else(|| "recusado".to_owned(), |code| code.to_string());
    let message = first
        .and_then(|error| error["message"].as_str())
        .unwrap_or("the gateway refused the request");
    Refusal::new(&code, message)
}

fn presence_of(key: &str) -> &'static str {
    match key {
        "in_person" => "Presence",
        "internet" => "Internet",
        "telephone" => "Telephone",
        "home_delivery" => "Delivery",
        "outside_the_establishment" => "PresenceOutsideEstablishment",
        "other" => "OthersNonPresenceOperation",
        _ => "None",
    }
}

fn method_of(form: PaymentForm) -> &'static str {
    match form {
        PaymentForm::Cash => "Cash",
        PaymentForm::Cheque => "Cheque",
        PaymentForm::CreditCard => "CreditCard",
        PaymentForm::DebitCard => "DebitCard",
        PaymentForm::StoreCredit => "StoreCredict",
        PaymentForm::BankSlip => "BankBill",
        PaymentForm::BankDeposit => "BankDeposit",
        PaymentForm::Pix => "InstantPayment",
        PaymentForm::Transfer => "WireTransfer",
        PaymentForm::NoPayment => "WithoutPayment",
        PaymentForm::Other => "Others",
    }
}

/// A moment in UTC as the specification asks for it: `2026-11-09T17:30:00Z`.
fn utc_timestamp(unix: i64) -> Option<String> {
    let day = conta_verde_domain::CalendarDate::from_unix_days(unix.div_euclid(86_400))?;
    let seconds = unix.rem_euclid(86_400);
    Some(format!(
        "{day}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    ))
}

impl FiscalGateway for NfeIoGateway {
    fn key(&self) -> &str {
        "nfeio"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn submit_nfe(&self, request: &NfeRequest) -> Result<Submission, GatewayError> {
        self.submit(request).await
    }

    async fn query_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<DocumentOutcome, GatewayError> {
        let (company, _) = self.target(issuer)?;
        let Some(id) = self.invoice_id(issuer, reference).await? else {
            return Ok(DocumentOutcome::Unknown);
        };
        match self.invoice(&company, &id).await? {
            Some(invoice) => self.outcome_of(&company, &invoice).await,
            None => Ok(DocumentOutcome::Unknown),
        }
    }

    async fn cancel_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let (company, _) = self.target(issuer)?;
        let Some(id) = self.invoice_id(issuer, reference).await? else {
            return Ok(EventOutcome::Refused(Refusal::new(
                "nao_encontrada",
                "the gateway has no document with this reference",
            )));
        };
        let answer = self
            .call(
                Method::DELETE,
                &format!("{}/{id}", self.invoices(&company)),
                &[("reason", reason)],
                None,
            )
            .await?;
        Self::queued_or_refused(&answer)
    }

    async fn correct_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        text: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let (company, _) = self.target(issuer)?;
        let Some(id) = self.invoice_id(issuer, reference).await? else {
            return Ok(EventOutcome::Refused(Refusal::new(
                "nao_encontrada",
                "the gateway has no document with this reference",
            )));
        };
        let answer = self
            .call(
                Method::PUT,
                &format!("{}/{id}/correctionletter", self.invoices(&company)),
                &[],
                Some(json!({ "reason": text }).to_string()),
            )
            .await?;
        Self::queued_or_refused(&answer)
    }

    async fn query_event(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        event: EventQuery,
    ) -> Result<EventOutcome, GatewayError> {
        let (company, _) = self.target(issuer)?;
        let Some(id) = self.invoice_id(issuer, reference).await? else {
            return Ok(EventOutcome::Queued);
        };
        match event {
            EventQuery::Cancellation => {
                // A cancelled document is the answer. One still authorized may be refused or not yet done: the specification
                // gives no refusal to read, so it stays queued and the person gives up on it after checking (ADR 0079).
                let cancelled = self
                    .invoice(&company, &id)
                    .await?
                    .is_some_and(|invoice| invoice["status"].as_str() == Some("Cancelled"));
                Ok(if cancelled {
                    EventOutcome::Accepted(AcceptedEvent {
                        protocol: None,
                        number: None,
                        xml: None,
                        pdf: None,
                    })
                } else {
                    EventOutcome::Queued
                })
            }
            EventQuery::Correction { accepted_before } => {
                let answer = self
                    .call(
                        Method::GET,
                        &format!("{}/{id}/events", self.invoices(&company)),
                        &[("limit", "20")],
                        None,
                    )
                    .await?;
                if !answer.status.is_success() {
                    return Err(GatewayError::Unavailable(format!(
                        "the events answered {}",
                        answer.status
                    )));
                }
                let letters = answer.body["events"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|event| event["type"].as_str().is_some_and(is_correction_letter))
                    .count();
                Ok(if letters > usize::from(accepted_before) {
                    EventOutcome::Accepted(AcceptedEvent {
                        protocol: None,
                        number: None,
                        xml: None,
                        pdf: None,
                    })
                } else {
                    EventOutcome::Queued
                })
            }
        }
    }

    async fn invalidate_nfe_numbers(
        &self,
        issuer: &FiscalSettings,
        series: u16,
        first: u32,
        last: u32,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let (company, environment) = self.target(issuer)?;
        let body = json!({
            "environment": environment,
            "serie": series,
            "state": issuer.address().state(),
            "beginNumber": first,
            "lastNumber": last,
            "reason": reason,
        });
        let answer = self
            .call(
                Method::POST,
                &format!("{}/disablement", self.invoices(&company)),
                &[],
                Some(body.to_string()),
            )
            .await?;
        match answer.status {
            // The range endpoint answers at once and repeating it is safe: the status carries the outcome.
            status if status.is_success() => Ok(EventOutcome::Accepted(AcceptedEvent {
                protocol: None,
                number: None,
                xml: None,
                pdf: None,
            })),
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
                Ok(EventOutcome::Refused(refusal_of(&answer.body)))
            }
            status => Err(GatewayError::Unavailable(format!(
                "the invalidation answered {status}"
            ))),
        }
    }
    // NFE.io's service invoices (NFS-e) are **not built**, and not because the gateway lacks them: its published
    // reference does not give what an adapter needs to promise that a service invoice is never issued twice ([ADR 0078],
    // [ADR 0081]). The listing takes a period, a page and no filter by our reference or `externalId` that the reference
    // documents, the authentication header and the answer's fields are not documented, and the ADR 0079 rule is that an
    // adapter that cannot find what it made before submitting never submits. They are refused with that reason, so no
    // document is sent: the installation that wants NFS-e uses another gateway until this one is read from a file
    // that settles those points.
    async fn submit_nfse(
        &self,
        _request: &NfseRequest,
    ) -> Result<Submission<AuthorizedNfse>, GatewayError> {
        Err(GatewayError::Unsupported(NFSE_NOT_BUILT))
    }

    async fn query_nfse(
        &self,
        _issuer: &conta_verde_domain::FiscalSettings,
        _reference: &str,
    ) -> Result<DocumentOutcome<AuthorizedNfse>, GatewayError> {
        Err(GatewayError::Unsupported(NFSE_NOT_BUILT))
    }

    async fn cancel_nfse(
        &self,
        _issuer: &conta_verde_domain::FiscalSettings,
        _reference: &str,
        _reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        Err(GatewayError::Unsupported(NFSE_NOT_BUILT))
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::Router;
    use axum::extract::{Path, Query, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use conta_verde_application::NfeRequest;
    use conta_verde_domain::{Address, AddressDraft, CalendarDate};

    use super::*;
    use crate::contract::{fiscal_issuer, nfe_request};

    const KEY: &str = "the-api-key";
    const CNPJ: &str = "11222333000181";
    const COMPANY: &str = "company-test";

    #[derive(Clone)]
    struct Stored {
        id: String,
        marker: String,
        status: &'static str,
        questions: u32,
        cancel_asked: bool,
        letters: u32,
        letters_visible: u32,
    }

    #[derive(Default)]
    struct Stand {
        invoices: Vec<Stored>,
        bodies: Vec<String>,
        posts: u32,
        search_fails: bool,
        answer_without_id: bool,
        disablements: Vec<Value>,
        disablement_status: u16,
        file_base: String,
    }

    type Shared = Arc<Mutex<Stand>>;

    fn allowed(headers: &HeaderMap) -> bool {
        headers
            .get("authorization")
            .is_some_and(|value| value == KEY)
    }

    fn key_of(number: u32) -> String {
        AccessKey::from_parts(
            35,
            CalendarDate::parse("2026-11-09").unwrap(),
            CNPJ,
            55,
            1,
            number,
            1,
            10_000_000 + number,
        )
        .unwrap()
        .as_str()
        .to_owned()
    }

    fn view(stored: &Stored) -> Value {
        let mut invoice = json!({
            "id": stored.id,
            "status": stored.status,
            "createdOn": format!("2026-11-09T17:30:{:02}Z", stored.id.len() % 60),
            "additionalInformation": { "taxpayer": format!("Ref. {}", stored.marker) },
        });
        if matches!(stored.status, "Issued" | "Cancelled") {
            invoice["number"] = json!(7);
            invoice["serie"] = json!(1);
            invoice["authorization"] = json!({ "accessKey": key_of(7), "message": "Autorizado" });
        }
        invoice
    }

    async fn serve(stand: Shared) -> SocketAddr {
        stand.lock().unwrap().disablement_status = 204;
        let base = "/v2/companies/{company}/productinvoices";
        let router = Router::new()
            .route(
                base,
                get(
                    |State(stand): State<Shared>,
                     headers: HeaderMap,
                     Query(query): Query<HashMap<String, String>>| async move {
                        if !allowed(&headers) {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        let stand = stand.lock().unwrap();
                        if stand.search_fails {
                            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                        }
                        let wanted = query.get("q").cloned().unwrap_or_default();
                        let found: Vec<Value> = stand
                            .invoices
                            .iter()
                            .filter(|stored| wanted.contains(&stored.marker))
                            .map(view)
                            .collect();
                        axum::Json(json!({ "productInvoices": found, "hasMore": false }))
                            .into_response()
                    },
                )
                .post(
                    |State(stand): State<Shared>, headers: HeaderMap, body: String| async move {
                        if !allowed(&headers) {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        let mut stand = stand.lock().unwrap();
                        stand.posts += 1;
                        let sent: Value = serde_json::from_str(&body).unwrap();
                        stand.bodies.push(body);
                        let text = sent["additionalInformation"]["taxpayer"]
                            .as_str()
                            .unwrap_or_default();
                        let marker = text.trim_start_matches("Ref. ").to_owned();
                        let id = format!("inv-{}", stand.invoices.len() + 1);
                        stand.invoices.push(Stored {
                            id: id.clone(),
                            marker,
                            status: "Processing",
                            questions: 0,
                            cancel_asked: false,
                            letters: 0,
                            letters_visible: 0,
                        });
                        if stand.answer_without_id {
                            axum::Json(json!({ "status": "queued" })).into_response()
                        } else {
                            axum::Json(json!({ "id": id })).into_response()
                        }
                    },
                ),
            )
            .route(
                "/v2/companies/{company}/productinvoices/{id}",
                get(
                    |State(stand): State<Shared>, headers: HeaderMap, Path((_, id)): Path<(String, String)>| async move {
                        if !allowed(&headers) {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        let mut stand = stand.lock().unwrap();
                        let Some(stored) = stand.invoices.iter_mut().find(|stored| stored.id == id) else {
                            return StatusCode::NOT_FOUND.into_response();
                        };
                        stored.questions += 1;
                        if stored.status == "Processing" && stored.questions >= 2 {
                            stored.status = "Issued";
                        }
                        if stored.cancel_asked && stored.status == "Issued" {
                            stored.status = "Cancelled";
                        }
                        axum::Json(view(stored)).into_response()
                    },
                )
                .delete(
                    |State(stand): State<Shared>,
                     headers: HeaderMap,
                     Path((_, id)): Path<(String, String)>,
                     Query(query): Query<HashMap<String, String>>| async move {
                        if !allowed(&headers) {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        if query.get("reason").is_none_or(|reason| reason.len() < 15) {
                            return (
                                StatusCode::BAD_REQUEST,
                                axum::Json(json!({ "errors": [{ "code": 400, "message": "reason is too short" }] })),
                            )
                                .into_response();
                        }
                        let mut stand = stand.lock().unwrap();
                        match stand.invoices.iter_mut().find(|stored| stored.id == id) {
                            Some(stored) => {
                                stored.cancel_asked = true;
                                StatusCode::NO_CONTENT.into_response()
                            }
                            None => StatusCode::NOT_FOUND.into_response(),
                        }
                    },
                ),
            )
            .route(
                "/v2/companies/{company}/productinvoices/{id}/{what}",
                get(
                    |State(stand): State<Shared>, Path((_, id, what)): Path<(String, String, String)>| async move {
                        let stand = stand.lock().unwrap();
                        match what.as_str() {
                            "xml" | "pdf" => axum::Json(json!({ "uri": format!("{}/files/{id}.{what}", stand.file_base) })).into_response(),
                            "events" => {
                                let events: Vec<Value> = stand
                                    .invoices
                                    .iter()
                                    .find(|stored| stored.id == id)
                                    .map(|stored| (0..stored.letters_visible).map(|n| json!({ "type": "CorrectionLetter", "sequence": n + 1 })).collect())
                                    .unwrap_or_default();
                                axum::Json(json!({ "events": events, "hasMore": false })).into_response()
                            }
                            _ => StatusCode::NOT_FOUND.into_response(),
                        }
                    },
                )
                .put(
                    |State(stand): State<Shared>, Path((_, id, what)): Path<(String, String, String)>, body: String| async move {
                        let sent: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                        if what != "correctionletter" || sent["reason"].as_str().is_none_or(|text| text.len() < 15) {
                            return (
                                StatusCode::BAD_REQUEST,
                                axum::Json(json!({ "errors": [{ "code": 400, "message": "the letter is too short" }] })),
                            )
                                .into_response();
                        }
                        let mut stand = stand.lock().unwrap();
                        if let Some(stored) = stand.invoices.iter_mut().find(|stored| stored.id == id) {
                            stored.letters += 1;
                        }
                        StatusCode::NO_CONTENT.into_response()
                    },
                ),
            )
            .route(
                "/v2/companies/{company}/productinvoices/disablement",
                post(|State(stand): State<Shared>, body: String| async move {
                    let mut stand = stand.lock().unwrap();
                    stand.disablements.push(serde_json::from_str(&body).unwrap());
                    match stand.disablement_status {
                        400 => (StatusCode::BAD_REQUEST, axum::Json(json!({ "errors": [{ "code": 40001, "message": "cStat 241" }] }))).into_response(),
                        status => StatusCode::from_u16(status).unwrap().into_response(),
                    }
                }),
            )
            .route(
                "/files/{name}",
                get(|Path(name): Path<String>| async move {
                    format!("<file of {name}/>").into_response()
                }),
            )
            .with_state(stand.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        stand.lock().unwrap().file_base = format!("http://{addr}");
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        addr
    }

    fn gateway(addr: SocketAddr, confirmed: bool) -> NfeIoGateway {
        NfeIoGateway::new(
            Some(&format!("http://{addr}")),
            KEY,
            Some(COMPANY),
            Some("company-production"),
            CNPJ,
            Arc::new(StaticCityCodes::default().with("SP", "São Paulo", "3550308")),
        )
        .unwrap()
        .with_search_confirmed(confirmed)
    }

    fn homologation() -> FiscalSettings {
        fiscal_issuer(CNPJ, FiscalEnvironment::Homologation)
    }

    fn request(reference: &str) -> NfeRequest {
        let mut request = nfe_request(&homologation(), reference);
        request.recipient.address = Some(
            Address::new(AddressDraft {
                postal_code: "01310100".into(),
                street: "Rua Augusta".into(),
                number: Some("10".into()),
                complement: None,
                district: Some("Consolação".into()),
                city: "SÃO PAULO".into(),
                state: "SP".into(),
            })
            .unwrap(),
        );
        request
    }

    #[tokio::test]
    async fn an_nfse_is_refused_with_the_reason_and_nothing_is_sent() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        let gateway = gateway(addr, true);
        let issuer = crate::contract::fiscal_service_issuer(CNPJ, FiscalEnvironment::Homologation);
        let asked = crate::contract::nfse_request(&issuer, "ref-nfse", 1);
        for answer in [
            gateway.submit_nfse(&asked).await.map(|_| ()),
            gateway.query_nfse(&issuer, "ref-nfse").await.map(|_| ()),
            gateway
                .cancel_nfse(&issuer, "ref-nfse", "Serviço não prestado")
                .await
                .map(|_| ()),
        ] {
            assert!(matches!(
                answer,
                Err(GatewayError::Unsupported(reason)) if reason.contains("another gateway")
            ));
        }
        let stand = stand.lock().unwrap();
        assert_eq!(stand.posts, 0);
        assert!(stand.bodies.is_empty(), "nothing reached the gateway");
    }

    #[tokio::test]
    async fn issues_a_document_it_names_and_follows_it_to_authorization_with_its_files() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        let gateway = gateway(addr, false);
        let submission = gateway.submit_nfe(&request("ref-1")).await.unwrap();
        assert_eq!(submission.reference.as_deref(), Some("nfeio:inv-1"));
        assert_eq!(submission.outcome, DocumentOutcome::Processing);

        // The body: our reference on the document, the tax number and every amount as JSON numbers, the municipality's code.
        let body = stand.lock().unwrap().bodies[0].clone();
        assert!(body.contains("\"taxpayer\":\"Ref. cv-ref-1\""), "{body}");
        assert!(body.contains("\"federalTaxNumber\":52998224725"), "{body}");
        assert!(body.contains("\"totalAmount\":47.23"), "{body}");
        assert!(body.contains("\"unitAmount\":0.4723"), "{body}");
        assert!(body.contains("\"cfop\":5102"), "{body}");
        assert!(body.contains("\"code\":\"3550308\""), "{body}");
        assert!(body.contains("\"method\":\"InstantPayment\""), "{body}");
        assert!(
            body.contains("\"destination\":\"Internal_Operation\""),
            "{body}"
        );
        assert!(
            body.contains("\"issuedOn\":\"2026-11-09T17:30:00Z\""),
            "{body}"
        );

        let named = submission.reference.unwrap();
        let first = gateway.query_nfe(&homologation(), &named).await.unwrap();
        assert_eq!(first, DocumentOutcome::Processing);
        let DocumentOutcome::Authorized(document) =
            gateway.query_nfe(&homologation(), &named).await.unwrap()
        else {
            panic!("authorized was expected");
        };
        assert_eq!(document.number, 7);
        assert_eq!(document.series, 1);
        assert_eq!(document.access_key.as_str(), key_of(7));
        assert_eq!(document.xml, b"<file of inv-1.xml/>");
        assert_eq!(document.pdf.as_deref(), Some(&b"<file of inv-1.pdf/>"[..]));
    }

    #[tokio::test]
    async fn finds_what_it_already_made_before_it_submits_and_never_submits_twice() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        let gateway = gateway(addr, false);
        let first = gateway.submit_nfe(&request("ref-2")).await.unwrap();
        let again = gateway.submit_nfe(&request("ref-2")).await.unwrap();
        assert_eq!(first.reference, again.reference);
        assert_eq!(stand.lock().unwrap().posts, 1);
        // Asked about by our own id (the name was never stored), it is found the same way.
        assert_eq!(
            gateway.query_nfe(&homologation(), "ref-2").await.unwrap(),
            DocumentOutcome::Processing
        );
        assert_eq!(
            gateway
                .query_nfe(&homologation(), "nothing-made")
                .await
                .unwrap(),
            DocumentOutcome::Unknown
        );
    }

    #[tokio::test]
    async fn a_search_that_fails_keeps_it_from_submitting_at_all() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        stand.lock().unwrap().search_fails = true;
        let error = gateway(addr, false)
            .submit_nfe(&request("ref-3"))
            .await
            .unwrap_err();
        assert!(matches!(error, GatewayError::Unavailable(_)), "{error:?}");
        assert_eq!(stand.lock().unwrap().posts, 0, "nothing was submitted");
    }

    #[tokio::test]
    async fn an_answer_without_the_id_is_found_by_the_search() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        stand.lock().unwrap().answer_without_id = true;
        let submission = gateway(addr, false)
            .submit_nfe(&request("ref-4"))
            .await
            .unwrap();
        assert_eq!(submission.reference.as_deref(), Some("nfeio:inv-1"));
        assert_eq!(stand.lock().unwrap().posts, 1);
    }

    #[tokio::test]
    async fn a_cancellation_and_a_correction_letter_are_queued_and_learned_by_asking_again() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        let gateway = gateway(addr, false);
        let named = gateway
            .submit_nfe(&request("ref-5"))
            .await
            .unwrap()
            .reference
            .unwrap();
        // Authorized after two questions.
        gateway.query_nfe(&homologation(), &named).await.unwrap();
        gateway.query_nfe(&homologation(), &named).await.unwrap();

        let letter = "Corrige o endereço de entrega";
        assert_eq!(
            gateway
                .correct_nfe(&homologation(), &named, letter)
                .await
                .unwrap(),
            EventOutcome::Queued
        );
        let waiting = gateway
            .query_event(
                &homologation(),
                &named,
                EventQuery::Correction { accepted_before: 0 },
            )
            .await
            .unwrap();
        assert_eq!(waiting, EventOutcome::Queued);
        stand.lock().unwrap().invoices[0].letters_visible = 1;
        let done = gateway
            .query_event(
                &homologation(),
                &named,
                EventQuery::Correction { accepted_before: 0 },
            )
            .await
            .unwrap();
        assert!(matches!(done, EventOutcome::Accepted(_)));
        let next = gateway
            .query_event(
                &homologation(),
                &named,
                EventQuery::Correction { accepted_before: 1 },
            )
            .await
            .unwrap();
        assert_eq!(
            next,
            EventOutcome::Queued,
            "the second letter is not the first one"
        );

        assert!(matches!(
            gateway.correct_nfe(&homologation(), &named, "curta").await.unwrap(),
            EventOutcome::Refused(refusal) if refusal.code == "400"
        ));
        assert_eq!(
            gateway
                .cancel_nfe(&homologation(), &named, "Pedido feito em duplicidade")
                .await
                .unwrap(),
            EventOutcome::Queued
        );
        assert!(matches!(
            gateway
                .cancel_nfe(&homologation(), &named, "curto")
                .await
                .unwrap(),
            EventOutcome::Refused(_)
        ));
        // The first question after the request finds it cancelled; the document itself says so too.
        let cancelled = gateway
            .query_event(&homologation(), &named, EventQuery::Cancellation)
            .await
            .unwrap();
        assert!(matches!(cancelled, EventOutcome::Accepted(_)));
        assert!(matches!(
            gateway.query_nfe(&homologation(), &named).await.unwrap(),
            DocumentOutcome::Cancelled { cancellation_xml, .. } if cancellation_xml.is_empty()
        ));
    }

    #[tokio::test]
    async fn a_range_of_numbers_is_invalidated_at_once_and_a_refusal_is_an_answer() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        let gateway = gateway(addr, false);
        let reason = "Numeração pulada por falha do sistema";
        assert!(matches!(
            gateway
                .invalidate_nfe_numbers(&homologation(), 1, 10, 12, reason)
                .await
                .unwrap(),
            EventOutcome::Accepted(_)
        ));
        let sent = stand.lock().unwrap().disablements[0].clone();
        assert_eq!(sent["environment"], "Test");
        assert_eq!(sent["beginNumber"], 10);
        assert_eq!(sent["lastNumber"], 12);
        assert_eq!(sent["state"], "SP");
        stand.lock().unwrap().disablement_status = 400;
        assert!(matches!(
            gateway.invalidate_nfe_numbers(&homologation(), 1, 10, 12, reason).await.unwrap(),
            EventOutcome::Refused(refusal) if refusal.code == "40001"
        ));
        stand.lock().unwrap().disablement_status = 503;
        assert!(matches!(
            gateway
                .invalidate_nfe_numbers(&homologation(), 1, 10, 12, reason)
                .await,
            Err(GatewayError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn refuses_what_it_cannot_do_safely_before_it_asks_anything() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        let gateway = gateway(addr, false);
        // Production waits for the operator to confirm the search in the sandbox.
        let production = fiscal_issuer(CNPJ, FiscalEnvironment::Production);
        let mut for_production = request("ref-6");
        for_production.issuer = production;
        assert!(matches!(
            gateway.submit_nfe(&for_production).await,
            Err(GatewayError::Unsupported(reason)) if reason.contains("NFEIO_SEARCH_CONFIRMED")
        ));
        // Another company's issuer, a recipient with no address, no code for the municipality, an alphanumeric document.
        let other = fiscal_issuer("11444777000161", FiscalEnvironment::Homologation);
        assert_eq!(
            gateway
                .submit_nfe(&nfe_request(&other, "x"))
                .await
                .unwrap_err(),
            GatewayError::WrongIssuer
        );
        assert!(matches!(
            gateway
                .submit_nfe(&nfe_request(&homologation(), "ref-7"))
                .await,
            Err(GatewayError::Unsupported(_))
        ));
        let mut unknown_city = request("ref-8");
        unknown_city.recipient.address = Some(
            Address::new(AddressDraft {
                postal_code: "01310100".into(),
                street: "Rua A".into(),
                number: None,
                complement: None,
                district: Some("Centro".into()),
                city: "Lugar Nenhum".into(),
                state: "SP".into(),
            })
            .unwrap(),
        );
        assert!(matches!(
            gateway.submit_nfe(&unknown_city).await,
            Err(GatewayError::Unsupported(reason)) if reason.contains("IBGE")
        ));
        assert_eq!(stand.lock().unwrap().posts, 0);
    }

    #[tokio::test]
    async fn a_file_from_a_host_the_gateway_did_not_name_is_not_followed() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        let gateway = gateway(addr, false);
        let named = gateway
            .submit_nfe(&request("ref-9"))
            .await
            .unwrap()
            .reference
            .unwrap();
        gateway.query_nfe(&homologation(), &named).await.unwrap();
        stand.lock().unwrap().file_base = "https://evil.example.com".to_owned();
        let error = gateway
            .query_nfe(&homologation(), &named)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, GatewayError::Unavailable(reason) if reason.contains("did not name")),
            "{error:?}"
        );
    }

    #[test]
    fn only_a_correction_letter_counts_as_one() {
        for yes in [
            "CorrectionLetter",
            "correction_letter_issued",
            "cce",
            "CCE_Authorized",
        ] {
            assert!(is_correction_letter(yes), "{yes}");
        }
        for no in [
            "Accepted",
            "Success",
            "Succeeded",
            "Cancellation",
            "Authorized",
        ] {
            assert!(!is_correction_letter(no), "{no}");
        }
    }

    #[tokio::test]
    async fn a_document_whose_number_was_declared_unused_is_not_the_one_found_and_is_made_again() {
        let stand = Shared::default();
        let addr = serve(stand.clone()).await;
        let gateway = gateway(addr, false);
        gateway.submit_nfe(&request("ref-10")).await.unwrap();
        stand.lock().unwrap().invoices[0].status = "Disabled";
        let again = gateway.submit_nfe(&request("ref-10")).await.unwrap();
        assert_eq!(again.reference.as_deref(), Some("nfeio:inv-2"));
        assert_eq!(stand.lock().unwrap().posts, 2);
    }

    #[tokio::test]
    async fn folds_names_the_way_municipalities_are_compared() {
        assert_eq!(fold("  São Paulo "), "sao paulo");
        assert_eq!(fold("BRASÍLIA"), "brasilia");
        let cities = StaticCityCodes::default().with("GO", "Goiânia", "5208707");
        assert_eq!(
            cities.code("GO", "GOIANIA").await.unwrap().as_deref(),
            Some("5208707")
        );
        assert_eq!(cities.code("SP", "Goiânia").await.unwrap(), None);
    }

    #[tokio::test]
    async fn reads_the_ibge_codes_of_a_state_once_and_refuses_a_name_it_cannot_tell_apart() {
        let asked = Arc::new(Mutex::new(0u32));
        let counter = asked.clone();
        let router = Router::new().route(
            "/api/ibge/municipios/v1/{state}",
            get(move |Path(state): Path<String>| {
                let counter = counter.clone();
                async move {
                    *counter.lock().unwrap() += 1;
                    assert_eq!(state, "SP");
                    axum::Json(json!([
                        { "nome": "SÃO PAULO", "codigo_ibge": "3550308" },
                        { "nome": "Igual", "codigo_ibge": "1" },
                        { "nome": "Igual", "codigo_ibge": "2" },
                    ]))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let cities = BrasilApiCityCodes::new(Some(&format!("http://{addr}"))).unwrap();
        assert_eq!(
            cities.code("SP", "São Paulo").await.unwrap().as_deref(),
            Some("3550308")
        );
        assert_eq!(cities.code("SP", "Igual").await.unwrap(), None);
        assert_eq!(cities.code("SP", "Outra").await.unwrap(), None);
        assert_eq!(*asked.lock().unwrap(), 1, "read once per state");
    }

    /// The homologation: against the real sandbox, with the operator's own key, company and CNPJ. It proves what the stand-in
    /// cannot: that the search finds the document we made (so a second submit makes none) and that an issue is accepted. It
    /// does nothing without `NFEIO_SANDBOX_API_KEY`, `NFEIO_SANDBOX_COMPANY_ID` and `NFEIO_SANDBOX_CNPJ`, and it needs the
    /// company registered with a certificate at the gateway. Run by hand; its result is what `NFEIO_SEARCH_CONFIRMED` says.
    #[tokio::test]
    #[ignore = "needs a NFE.io sandbox account"]
    async fn the_sandbox_finds_what_it_made_so_a_second_submit_makes_none() {
        let (Ok(key), Ok(company), Ok(cnpj)) = (
            std::env::var("NFEIO_SANDBOX_API_KEY"),
            std::env::var("NFEIO_SANDBOX_COMPANY_ID"),
            std::env::var("NFEIO_SANDBOX_CNPJ"),
        ) else {
            return;
        };
        let gateway = NfeIoGateway::new(
            None,
            &key,
            Some(&company),
            None,
            &cnpj,
            Arc::new(BrasilApiCityCodes::new(None).unwrap()),
        )
        .unwrap();
        let issuer = fiscal_issuer(&cnpj, FiscalEnvironment::Homologation);
        let reference = format!("sandbox-{}", uuid::Uuid::new_v4());
        let mut request = nfe_request(&issuer, &reference);
        request.recipient.address = Some(
            Address::new(AddressDraft {
                postal_code: "01310100".into(),
                street: "Avenida Paulista".into(),
                number: Some("1000".into()),
                complement: None,
                district: Some("Bela Vista".into()),
                city: "São Paulo".into(),
                state: "SP".into(),
            })
            .unwrap(),
        );
        let first = gateway.submit_nfe(&request).await.unwrap();
        let second = gateway.submit_nfe(&request).await.unwrap();
        assert!(first.reference.is_some());
        assert_eq!(
            first.reference, second.reference,
            "the second submit found the first document instead of making another"
        );
    }
}
