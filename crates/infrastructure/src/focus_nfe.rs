use std::time::Duration;

use conta_verde_application::{
    AcceptedEvent, AuthorizedDocument, AuthorizedNfse, Destination, DocumentOutcome, EventOutcome,
    EventQuery, FiscalGateway, GatewayError, NfeRequest, NfseRequest, Refusal, Submission,
};
use conta_verde_domain::{
    AccessKey, Amount, Decimal, FiscalEnvironment, FiscalSettings, IssTaxation, TaxRegime,
    fiscal_timestamp,
};
use reqwest::{Method, StatusCode};
use serde_json::{Map, Value, json};

use crate::brasil_api::client_without_redirects;
use crate::json_numbers::Numbers;

const HOMOLOGATION_URL: &str = "https://homologacao.focusnfe.com.br";
const PRODUCTION_URL: &str = "https://api.focusnfe.com.br";
const TIMEOUT: Duration = Duration::from_secs(30);
/// The biggest file taken from the gateway: an XML or a DANFE is a few hundred kilobytes.
const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

/// What one call came to: the status and the body as JSON (`null` when it had none).
struct Answer {
    status: StatusCode,
    body: Value,
}

/// A fiscal gateway that issues through [Focus NFe](https://focusnfe.com.br) ([ADR 0075]): the NF-e, its query,
/// cancellation, correction letter and the invalidation of numbers. The gateway keeps the company's certificate and
/// signs with it; the token of each environment and the issuer's CNPJ come from the environment of the server and are
/// never stored or logged, and a call for another CNPJ is refused before it is made.
///
/// **Everything about its API was read from its public documentation and none of it exercised**: the fields of the
/// request beyond the documented simple sale, how a document issued in contingency is reported, and where the files
/// are downloaded from are for the ignored sandbox test to settle.
#[derive(Clone)]
pub struct FocusNfeGateway {
    client: reqwest::Client,
    base_url: Option<String>,
    homologation_token: Option<String>,
    production_token: Option<String>,
    issuer_cnpj: String,
}

impl FocusNfeGateway {
    /// `base_url` replaces both hosts (a stand-in in a test). A token that is not given makes the calls of its
    /// environment unavailable.
    pub fn new(
        base_url: Option<&str>,
        homologation_token: Option<&str>,
        production_token: Option<&str>,
        issuer_cnpj: &str,
    ) -> Result<Self, reqwest::Error> {
        Self::with_timeout(
            base_url,
            homologation_token,
            production_token,
            issuer_cnpj,
            TIMEOUT,
        )
    }

    pub fn with_timeout(
        base_url: Option<&str>,
        homologation_token: Option<&str>,
        production_token: Option<&str>,
        issuer_cnpj: &str,
        timeout: Duration,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: client_without_redirects(timeout)?,
            base_url: base_url.map(|url| url.trim_end_matches('/').to_owned()),
            homologation_token: homologation_token.map(str::to_owned),
            production_token: production_token.map(str::to_owned),
            issuer_cnpj: issuer_cnpj.to_owned(),
        })
    }

    /// The host and the token of the issuer's environment, once the issuer is the one this gateway is configured for.
    fn target(&self, issuer: &FiscalSettings) -> Result<(String, &str), GatewayError> {
        if issuer.cnpj() != self.issuer_cnpj {
            return Err(GatewayError::WrongIssuer);
        }
        let (default_url, token) = match issuer.environment() {
            FiscalEnvironment::Homologation => (HOMOLOGATION_URL, &self.homologation_token),
            FiscalEnvironment::Production => (PRODUCTION_URL, &self.production_token),
        };
        let token = token.as_deref().ok_or_else(|| {
            GatewayError::Unavailable(format!(
                "no token is configured for the {} environment",
                issuer.environment().key()
            ))
        })?;
        Ok((
            self.base_url
                .clone()
                .unwrap_or_else(|| default_url.to_owned()),
            token,
        ))
    }

    /// One call. Only the status is ever logged: a body carries the customer's data, and a request the certificate's
    /// owner's.
    async fn call(
        &self,
        method: Method,
        url: &str,
        token: &str,
        body: Option<String>,
    ) -> Result<Answer, GatewayError> {
        // The token is the user and the password is empty, as the gateway documents.
        let mut request = self.client.request(method, url).basic_auth(token, Some(""));
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body);
        }
        let response = request.send().await.map_err(|error| {
            // Without the address: it holds the reference.
            GatewayError::Unavailable(format!("request failed: {}", error.without_url()))
        })?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|error| {
            GatewayError::Unavailable(format!("the answer was cut: {}", error.without_url()))
        })?;
        // An error may come without a body, but a success that is not JSON is no answer: reading it as a refusal
        // would show a cancellation the authority took as refused.
        let body = match serde_json::from_slice(&bytes) {
            Ok(body) => body,
            Err(_) if status.is_success() => {
                return Err(GatewayError::Unavailable(
                    "a success whose answer is not JSON".to_owned(),
                ));
            }
            Err(_) => Value::Null,
        };
        Ok(Answer { status, body })
    }

    /// A file the gateway made, by the path it answered with. Only a path on the gateway's own host is followed.
    async fn download(&self, base: &str, token: &str, path: &str) -> Result<Vec<u8>, GatewayError> {
        if !path.starts_with('/')
            || path.starts_with("//")
            || path.split('/').any(|segment| segment == "..")
        {
            return Err(GatewayError::Unavailable(
                "a file is somewhere other than a path on the gateway".to_owned(),
            ));
        }
        let response = self
            .client
            .get(format!("{base}{path}"))
            .basic_auth(token, Some(""))
            .send()
            .await
            .map_err(|error| {
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
        Ok(bytes.to_vec())
    }

    /// What an answer about a document means, with its files fetched when it is authorized or cancelled.
    async fn outcome_of(
        &self,
        base: &str,
        token: &str,
        answer: &Answer,
    ) -> Result<DocumentOutcome, GatewayError> {
        let body = &answer.body;
        match body["status"].as_str() {
            Some("processando_autorizacao") => Ok(DocumentOutcome::Processing),
            Some("autorizado") => Ok(DocumentOutcome::Authorized(
                self.authorized(base, token, body).await?,
            )),
            Some("cancelado") => {
                let document = self.authorized(base, token, body).await?;
                let path = text(body, "caminho_xml_cancelamento").ok_or_else(|| {
                    GatewayError::Unavailable("a cancelled document has no cancellation XML".into())
                })?;
                Ok(DocumentOutcome::Cancelled {
                    document,
                    cancellation_xml: self.download(base, token, &path).await?,
                })
            }
            Some("erro_autorizacao") => Ok(DocumentOutcome::Rejected(refusal_of(body))),
            Some("denegado" | "denegada") => Ok(DocumentOutcome::Denied(refusal_of(body))),
            other => Err(GatewayError::Unavailable(format!(
                "an answer with a status this adapter does not know: {}",
                other
                    .unwrap_or("none")
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(40)
                    .collect::<String>()
            ))),
        }
    }

    async fn authorized(
        &self,
        base: &str,
        token: &str,
        body: &Value,
    ) -> Result<AuthorizedDocument, GatewayError> {
        let missing = |field: &str| GatewayError::Unavailable(format!("the answer has no {field}"));
        let key = text(body, "chave_nfe").ok_or_else(|| missing("chave_nfe"))?;
        // The gateway writes the key with the `NFe` the XML's id has.
        let key = key.strip_prefix("NFe").unwrap_or(&key);
        let access_key = AccessKey::parse(key)
            .map_err(|_| GatewayError::Unavailable("the answer's access key is not one".into()))?;
        let number = whole(body, "numero").ok_or_else(|| missing("numero"))?;
        let series = whole(body, "serie").ok_or_else(|| missing("serie"))?;
        // What the key says must be what the answer says, and the key must be this issuer's: a document of another
        // company is never taken for ours.
        if access_key.issuer() != self.issuer_cnpj
            || access_key.number().parse::<u64>().ok() != Some(number)
            || access_key.series().parse::<u64>().ok() != Some(series)
        {
            return Err(GatewayError::Unavailable(
                "the answer's access key does not match its issuer, number and series".to_owned(),
            ));
        }
        let xml_path = text(body, "caminho_xml_nota_fiscal")
            .ok_or_else(|| missing("caminho_xml_nota_fiscal"))?;
        let pdf = match text(body, "caminho_danfe") {
            Some(path) => Some(self.download(base, token, &path).await?),
            None => None,
        };
        Ok(AuthorizedDocument {
            access_key,
            number: u32::try_from(number).map_err(|_| missing("numero"))?,
            series: u16::try_from(series).map_err(|_| missing("serie"))?,
            protocol: text(body, "protocolo"),
            xml: self.download(base, token, &xml_path).await?,
            pdf,
        })
    }

    /// What an event's answer means: taken (`accepted` is the status that says so) or refused, the files fetched.
    async fn event_of(
        &self,
        base: &str,
        token: &str,
        answer: &Answer,
        accepted: &str,
        files: &[&str],
        refusal_of: fn(&Value) -> Refusal,
    ) -> Result<EventOutcome, GatewayError> {
        match answer.status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(GatewayError::Unavailable("the token was refused".into()));
            }
            StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT => {
                return Err(GatewayError::Unavailable(format!("{}", answer.status)));
            }
            status if status.is_server_error() => {
                return Err(GatewayError::Unavailable(format!("{status}")));
            }
            status if status.is_client_error() => {
                return Ok(EventOutcome::Refused(refusal_of_error(&answer.body)));
            }
            _ => {}
        }
        let body = &answer.body;
        if body["status"].as_str() != Some(accepted) {
            return Ok(EventOutcome::Refused(refusal_of(body)));
        }
        let mut downloaded: Vec<Option<Vec<u8>>> = Vec::with_capacity(files.len());
        for field in files {
            downloaded.push(match text(body, field) {
                Some(path) => Some(self.download(base, token, &path).await?),
                None => None,
            });
        }
        let mut downloaded = downloaded.into_iter();
        Ok(EventOutcome::Accepted(AcceptedEvent {
            protocol: text(body, "protocolo"),
            number: whole(body, "numero_carta_correcao")
                .and_then(|number| u32::try_from(number).ok()),
            xml: downloaded.next().flatten(),
            pdf: downloaded.next().flatten(),
        }))
    }

    /// What an answer about an NFS-e means, with its files fetched when it is authorized or cancelled.
    async fn nfse_outcome_of(
        &self,
        base: &str,
        token: &str,
        answer: &Answer,
    ) -> Result<DocumentOutcome<AuthorizedNfse>, GatewayError> {
        let body = &answer.body;
        match body["status"].as_str() {
            Some("processando_autorizacao") => Ok(DocumentOutcome::Processing),
            Some("autorizado") => Ok(DocumentOutcome::Authorized(
                self.authorized_nfse(base, token, body).await?,
            )),
            Some("cancelado") => {
                let document = self.authorized_nfse(base, token, body).await?;
                // The cancellation's XML is in the documented example and not in its schema: a document cancelled
                // without it is cancelled all the same, and there is no file to keep.
                let cancellation_xml = match text(body, "caminho_xml_cancelamento")
                    .and_then(|path| own_path(base, &path))
                {
                    Some(path) => self.download(base, token, &path).await?,
                    None => Vec::new(),
                };
                Ok(DocumentOutcome::Cancelled {
                    document,
                    cancellation_xml,
                })
            }
            Some("erro_autorizacao") => Ok(DocumentOutcome::Rejected(refusal_of_nfse(body))),
            Some("negado") => Ok(DocumentOutcome::Denied(refusal_of_nfse(body))),
            other => Err(GatewayError::Unavailable(format!(
                "an answer with a status this adapter does not know: {}",
                other
                    .unwrap_or("none")
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(40)
                    .collect::<String>()
            ))),
        }
    }

    async fn authorized_nfse(
        &self,
        base: &str,
        token: &str,
        body: &Value,
    ) -> Result<AuthorizedNfse, GatewayError> {
        let missing = |field: &str| GatewayError::Unavailable(format!("the answer has no {field}"));
        let number = whole(body, "numero").ok_or_else(|| missing("numero"))?;
        // The XML is the fiscal record: without it the document is not taken as authorized.
        let xml_path = text(body, "caminho_xml_nota_fiscal")
            .and_then(|path| own_path(base, &path))
            .ok_or_else(|| missing("caminho_xml_nota_fiscal"))?;
        // The printed form may be somewhere the gateway does not host (a storage service): then it is not kept, and the
        // person has the XML and the authority's portal.
        let pdf = match text(body, "url_danfse").and_then(|url| own_path(base, &url)) {
            Some(path) => Some(self.download(base, token, &path).await?),
            None => None,
        };
        Ok(AuthorizedNfse {
            number,
            key: None,
            verification_code: text(body, "codigo_verificacao"),
            protocol: text(body, "protocolo"),
            xml: self.download(base, token, &xml_path).await?,
            pdf,
        })
    }

    async fn submit_service(
        &self,
        request: &NfseRequest,
    ) -> Result<DocumentOutcome<AuthorizedNfse>, GatewayError> {
        let (base, token) = self.target(&request.issuer)?;
        let reference = path_reference(&request.reference)?;
        let answer = self
            .call(
                Method::POST,
                &format!("{base}/v2/nfsen?ref={reference}"),
                token,
                Some(nfse_body(request)?),
            )
            .await?;
        match answer.status {
            StatusCode::ACCEPTED => Ok(DocumentOutcome::Processing),
            status if status.is_success() => self.nfse_outcome_of(&base, token, &answer).await,
            // A refusal of the request is told apart from "this reference is already known" by asking: the gateway's
            // own words for a duplicate are not documented, and a duplicate is the document it made, not a refusal.
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
                match self.query_nfse(&request.issuer, reference).await? {
                    DocumentOutcome::Unknown => failure(&answer).map(DocumentOutcome::Rejected),
                    known => Ok(known),
                }
            }
            _ => failure(&answer).map(DocumentOutcome::Rejected),
        }
    }

    async fn submit(&self, request: &NfeRequest) -> Result<DocumentOutcome, GatewayError> {
        let (base, token) = self.target(&request.issuer)?;
        if request.issuer.regime() != TaxRegime::SimplesNacional {
            return Err(GatewayError::Unsupported(
                "an NF-e for Lucro Presumido, which needs the tax calculation",
            ));
        }
        let reference = path_reference(&request.reference)?;
        let answer = self
            .call(
                Method::POST,
                &format!("{base}/v2/nfe?ref={reference}"),
                token,
                Some(nfe_body(request)?),
            )
            .await?;
        match answer.status {
            StatusCode::ACCEPTED => Ok(DocumentOutcome::Processing),
            status if status.is_success() => self.outcome_of(&base, token, &answer).await,
            // The reference is already known to the gateway: what it says is the state of that document.
            StatusCode::UNPROCESSABLE_ENTITY
                if matches!(
                    answer.body["codigo"].as_str(),
                    Some("already_processed" | "pending_operation")
                ) =>
            {
                self.query_nfe(&request.issuer, reference).await
            }
            _ => failure(&answer).map(DocumentOutcome::Rejected),
        }
    }
}

/// A string field of an answer, when it is a non-empty one.
fn text(body: &Value, field: &str) -> Option<String> {
    body[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// A whole number the gateway wrote as a number or as text.
fn whole(body: &Value, field: &str) -> Option<u64> {
    match &body[field] {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// Why a document was refused: the SEFAZ's code and text and the fields the gateway says are wrong.
fn refusal_of(body: &Value) -> Refusal {
    let mut message = text(body, "mensagem_sefaz")
        .or_else(|| text(body, "mensagem"))
        .unwrap_or_default();
    if let Some(errors) = body["erros"].as_array() {
        for error in errors.iter().take(5) {
            let field = error["campo"].as_str().unwrap_or("");
            let reason = error["mensagem"].as_str().unwrap_or("");
            message.push_str(&format!(" [{field}: {reason}]"));
        }
    }
    let code = text(body, "status_sefaz")
        .or_else(|| body["status_sefaz"].as_u64().map(|code| code.to_string()))
        .or_else(|| text(body, "codigo"))
        .unwrap_or_else(|| "recusado".to_owned());
    Refusal::new(&code, &message)
}

/// The refusal of a request the gateway rejected as a whole: `{"codigo", "mensagem", "erros"}`.
fn refusal_of_error(body: &Value) -> Refusal {
    let code = text(body, "codigo").unwrap_or_else(|| "recusado".to_owned());
    let mut refusal = refusal_of(body);
    refusal.code = Refusal::new(&code, "").code;
    refusal
}

/// The reason of an NFS-e that was not authorized or of a cancellation that was refused: each of the `erros` has a
/// `codigo`, a `mensagem` and a `correcao` that says how to fix it.
fn refusal_of_nfse(body: &Value) -> Refusal {
    let errors = body["erros"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let code = errors
        .first()
        .and_then(|error| text(error, "codigo"))
        .or_else(|| text(body, "codigo"))
        .unwrap_or_else(|| "recusado".to_owned());
    let mut message = errors
        .iter()
        .take(5)
        .map(|error| {
            let reason = error["mensagem"].as_str().unwrap_or("");
            match error["correcao"].as_str().filter(|fix| !fix.is_empty()) {
                Some(fix) => format!("{reason} ({fix})"),
                None => reason.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    if message.is_empty() {
        message = text(body, "mensagem").unwrap_or_default();
    }
    Refusal::new(&code, &message)
}

/// The path of a file the gateway says it holds, when it is on the gateway's own host: a path as it is, or a whole address
/// that starts with the gateway's own. Anything else (another host) is not followed.
fn own_path(base: &str, value: &str) -> Option<String> {
    if value.starts_with('/') {
        return Some(value.to_owned());
    }
    value
        .strip_prefix(base)
        .filter(|rest| rest.starts_with('/'))
        .map(str::to_owned)
}

/// The DPS of an NFS-e for the gateway's national endpoint, in the fields its documentation lists as required and the
/// ones the nature says. **None of the reform's fields is sent**, and none is invented: what the sandbox asks for beyond
/// this is for the ignored sandbox test to settle ([ADR 0081]).
fn nfse_body(request: &NfseRequest) -> Result<String, GatewayError> {
    let issuer = &request.issuer;
    let city = issuer.city_ibge_code().ok_or(GatewayError::Unsupported(
        "an NFS-e whose issuer has no municipality code",
    ))?;
    let issued_at =
        fiscal_timestamp(request.issued_at_unix, issuer.address().state()).ok_or_else(|| {
            GatewayError::Unavailable("the moment of issue is out of range".to_owned())
        })?;
    let numbers = Numbers::new();
    let mut body = Map::new();
    let mut put = |key: &str, value: Value| {
        body.insert(key.to_owned(), value);
    };
    let number = |text: &str, field: &'static str| {
        text.parse::<u64>().map_err(|_| {
            GatewayError::Unavailable(format!("{field} is not a number of the national layout"))
        })
    };
    put("data_emissao", json!(issued_at));
    put("serie_dps", json!(request.dps_series));
    put("numero_dps", json!(request.dps_number));
    put("data_competencia", json!(request.competence.to_string()));
    // The provider issues it (1), the layout's default.
    put("emitente_dps", json!(1));
    put(
        "codigo_municipio_emissora",
        json!(number(city, "codigo_municipio_emissora")?),
    );
    put("cnpj_prestador", json!(issuer.cnpj()));
    if let Some(registration) = issuer.municipal_registration() {
        put("inscricao_municipal_prestador", json!(registration));
    }
    // Only the Simples Nacional issues for now ([ADR 0073]): an optant that is not a MEI is ME/EPP (3), because the
    // settings refuse a MEI. The other regimes wait for the tax calculation.
    if issuer.regime() != TaxRegime::SimplesNacional {
        return Err(GatewayError::Unsupported(
            "an NFS-e for Lucro Presumido, which needs the tax calculation",
        ));
    }
    put("codigo_opcao_simples_nacional", json!(3));
    put(
        "regime_especial_tributacao",
        json!(request.special_regime.code()),
    );

    let recipient = &request.recipient;
    let key = if recipient.document.len() == 14 {
        "cnpj_tomador"
    } else {
        "cpf_tomador"
    };
    put(key, json!(recipient.document));
    if let Some(email) = &recipient.email {
        put("email_tomador", json!(email));
    }
    if let Some(address) = &recipient.address {
        put("cep_tomador", json!(address.postal_code()));
        put("logradouro_tomador", json!(address.street()));
        put("numero_tomador", json!(address.number().unwrap_or("S/N")));
        if let Some(complement) = address.complement() {
            put("complemento_tomador", json!(complement));
        }
        if let Some(district) = address.district() {
            put("bairro_tomador", json!(district));
        }
    }

    put(
        "codigo_municipio_prestacao",
        json!(request.service_city_code),
    );
    put(
        "codigo_tributacao_nacional_iss",
        json!(request.national_tax_code),
    );
    if let Some(code) = &request.municipal_service_code {
        put("codigo_tributacao_municipal_iss", json!(code));
    }
    put("descricao_servico", json!(request.description));
    put("valor_servico", numbers.of(request.gross));
    if request.discount.as_decimal() > Decimal::ZERO {
        put("desconto_incondicionado", numbers.of(request.discount));
    }
    put(
        "tributacao_iss",
        json!(match request.taxation {
            IssTaxation::Taxable => 1,
            IssTaxation::Immune => 2,
            IssTaxation::Export => 3,
            IssTaxation::NotIncident => 4,
        }),
    );
    if let Some(rate) = request.iss_rate {
        put("percentual_aliquota_relativa_municipio", numbers.of(rate));
    }
    Ok(numbers.render(&Value::Object(body)))
}

/// The body of an NF-e for the gateway, in the fields its documentation gives for a sale, with every amount as the
/// exact decimal it is.
fn nfe_body(request: &NfeRequest) -> Result<String, GatewayError> {
    let issuer = &request.issuer;
    let state = issuer.address().state();
    let issued_at = fiscal_timestamp(request.issued_at_unix, state).ok_or_else(|| {
        GatewayError::Unavailable("the moment of issue is out of range".to_owned())
    })?;
    let numbers = Numbers::new();
    let money = |amount: &Amount| numbers.of(amount);
    let mut body = Map::new();
    let mut put = |key: &str, value: Value| {
        body.insert(key.to_owned(), value);
    };
    put("natureza_operacao", json!(request.nature));
    put("data_emissao", json!(issued_at));
    // Output (1) and a normal finality (1): the documents this product issues.
    put("tipo_documento", json!(1));
    put("finalidade_emissao", json!(1));
    put(
        "local_destino",
        json!(match request.destination {
            Destination::InState => 1,
            Destination::Interstate => 2,
            Destination::Foreign => 3,
        }),
    );
    put("consumidor_final", json!(u8::from(request.final_consumer)));
    put("presenca_comprador", json!(request.presence.code()));
    put("cnpj_emitente", json!(issuer.cnpj()));
    put("nome_emitente", json!(issuer.legal_name()));
    if let Some(registration) = issuer.state_registration() {
        put("inscricao_estadual_emitente", json!(registration));
    }
    put("uf_emitente", json!(state));
    put(
        "regime_tributario_emitente",
        json!(match issuer.regime() {
            TaxRegime::SimplesNacional => 1,
            TaxRegime::LucroPresumido => 3,
        }),
    );

    let recipient = &request.recipient;
    if let Some(document) = &recipient.document {
        let key = if document.len() == 14 {
            "cnpj_destinatario"
        } else {
            "cpf_destinatario"
        };
        put(key, json!(document));
    }
    put("nome_destinatario", json!(recipient.name));
    if let Some(email) = &recipient.email {
        put("email_destinatario", json!(email));
    }
    if let Some(address) = &recipient.address {
        put("logradouro_destinatario", json!(address.street()));
        put(
            "numero_destinatario",
            json!(address.number().unwrap_or("S/N")),
        );
        if let Some(complement) = address.complement() {
            put("complemento_destinatario", json!(complement));
        }
        if let Some(district) = address.district() {
            put("bairro_destinatario", json!(district));
        }
        put("municipio_destinatario", json!(address.city()));
        put("uf_destinatario", json!(address.state()));
        put("cep_destinatario", json!(address.postal_code()));
    }
    match &recipient.state_registration {
        Some(registration) => {
            // A taxpayer of the ICMS (1).
            put("indicador_inscricao_estadual_destinatario", json!(1));
            put("inscricao_estadual_destinatario", json!(registration));
        }
        // Not a taxpayer (9).
        None => put("indicador_inscricao_estadual_destinatario", json!(9)),
    }

    // No freight (9).
    put("modalidade_frete", json!(9));
    put("valor_produtos", money(&request.total_products));
    put("valor_desconto", money(&request.total_discount));
    put("valor_total", money(&request.total));
    put(
        "formas_pagamento",
        Value::Array(
            request
                .payments
                .iter()
                .map(|(form, amount)| {
                    json!({ "forma_pagamento": form.code(), "valor_pagamento": money(amount) })
                })
                .collect(),
        ),
    );
    put(
        "items",
        Value::Array(
            request
                .items
                .iter()
                .map(|item| {
                    let mut line = json!({
                        "numero_item": item.number,
                        "codigo_produto": item.code,
                        "descricao": item.description,
                        "cfop": item.cfop,
                        "codigo_ncm": item.ncm,
                        "unidade_comercial": item.unit,
                        "quantidade_comercial": numbers.of(item.quantity),
                        "valor_unitario_comercial": numbers.of(item.unit_price),
                        "unidade_tributavel": item.unit,
                        "quantidade_tributavel": numbers.of(item.quantity),
                        "valor_unitario_tributavel": numbers.of(item.unit_price),
                        "valor_bruto": money(&item.gross),
                        "icms_origem": item.origin,
                        "icms_situacao_tributaria": item.icms_code,
                        "pis_situacao_tributaria": item.pis_code,
                        "cofins_situacao_tributaria": item.cofins_code,
                    });
                    if item.discount.as_decimal() > Decimal::ZERO {
                        line["valor_desconto"] = money(&item.discount);
                    }
                    line
                })
                .collect(),
        ),
    );
    Ok(numbers.render(&Value::Object(body)))
}

/// What the gateway says of a request it did not take as a document: a refusal the person can read, or a failure to
/// ask again about. The status decides which.
fn failure(answer: &Answer) -> Result<Refusal, GatewayError> {
    match answer.status {
        // Our token being refused, a slow answer and the rate limit are not the document's fault: asking again
        // later, or the operator fixing the token, is the remedy.
        StatusCode::UNAUTHORIZED
        | StatusCode::FORBIDDEN
        | StatusCode::REQUEST_TIMEOUT
        | StatusCode::TOO_MANY_REQUESTS => {
            Err(GatewayError::Unavailable(format!("{}", answer.status)))
        }
        // The gateway read the request and refused it. Any other `4xx` (a route that is not there, a method it does not
        // take) is a problem of the call, not of the document.
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
            Ok(refusal_of_error(&answer.body))
        }
        status => Err(GatewayError::Unavailable(format!("{status}"))),
    }
}

/// A reference in a path: the characters our ids have and nothing that could change the route.
fn path_reference(reference: &str) -> Result<&str, GatewayError> {
    if !reference.is_empty()
        && reference
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Ok(reference)
    } else {
        Err(GatewayError::Unsupported(
            "a reference that is not a plain id",
        ))
    }
}

impl FiscalGateway for FocusNfeGateway {
    fn key(&self) -> &str {
        "focus"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn submit_nfe(&self, request: &NfeRequest) -> Result<Submission, GatewayError> {
        // Focus NFe keeps our reference, so it names nothing of its own.
        self.submit(request).await.map(Submission::of)
    }

    async fn query_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<DocumentOutcome, GatewayError> {
        let (base, token) = self.target(issuer)?;
        let reference = path_reference(reference)?;
        let answer = self
            .call(
                Method::GET,
                &format!("{base}/v2/nfe/{reference}"),
                token,
                None,
            )
            .await?;
        match answer.status {
            StatusCode::NOT_FOUND => Ok(DocumentOutcome::Unknown),
            status if status.is_success() => self.outcome_of(&base, token, &answer).await,
            // A question the gateway would not answer says nothing about the document: it is not a refusal of it.
            status => Err(GatewayError::Unavailable(format!("{status}"))),
        }
    }

    async fn cancel_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let (base, token) = self.target(issuer)?;
        let reference = path_reference(reference)?;
        let answer = self
            .call(
                Method::DELETE,
                &format!("{base}/v2/nfe/{reference}"),
                token,
                Some(json!({ "justificativa": reason }).to_string()),
            )
            .await?;
        self.event_of(
            &base,
            token,
            &answer,
            "cancelado",
            &["caminho_xml_cancelamento"],
            refusal_of,
        )
        .await
    }

    async fn correct_nfe(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        text: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let (base, token) = self.target(issuer)?;
        let reference = path_reference(reference)?;
        let answer = self
            .call(
                Method::POST,
                &format!("{base}/v2/nfe/{reference}/carta_correcao"),
                token,
                Some(json!({ "correcao": text }).to_string()),
            )
            .await?;
        self.event_of(
            &base,
            token,
            &answer,
            "autorizado",
            &["caminho_xml_carta_correcao", "caminho_pdf_carta_correcao"],
            refusal_of,
        )
        .await
    }

    async fn query_event(
        &self,
        _issuer: &FiscalSettings,
        _reference: &str,
        _event: EventQuery,
    ) -> Result<EventOutcome, GatewayError> {
        // Focus NFe answers its events at once, so none is ever queued and none is asked about again.
        Err(GatewayError::Unsupported("asking again about an event"))
    }

    async fn invalidate_nfe_numbers(
        &self,
        issuer: &FiscalSettings,
        series: u16,
        first: u32,
        last: u32,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let (base, token) = self.target(issuer)?;
        let answer = self
            .call(
                Method::POST,
                &format!("{base}/v2/nfe/inutilizacao"),
                token,
                Some(
                    json!({
                        "cnpj": issuer.cnpj(),
                        "serie": series.to_string(),
                        "numero_inicial": first,
                        "numero_final": last,
                        "justificativa": reason,
                    })
                    .to_string(),
                ),
            )
            .await?;
        self.event_of(
            &base,
            token,
            &answer,
            "autorizado",
            &["caminho_xml"],
            refusal_of,
        )
        .await
    }

    async fn submit_nfse(
        &self,
        request: &NfseRequest,
    ) -> Result<Submission<AuthorizedNfse>, GatewayError> {
        // Focus NFe keeps our reference, so it names nothing of its own.
        self.submit_service(request).await.map(Submission::of)
    }

    async fn query_nfse(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
    ) -> Result<DocumentOutcome<AuthorizedNfse>, GatewayError> {
        let (base, token) = self.target(issuer)?;
        let reference = path_reference(reference)?;
        let answer = self
            .call(
                Method::GET,
                &format!("{base}/v2/nfsen/{reference}"),
                token,
                None,
            )
            .await?;
        match answer.status {
            StatusCode::NOT_FOUND => Ok(DocumentOutcome::Unknown),
            status if status.is_success() => self.nfse_outcome_of(&base, token, &answer).await,
            status => Err(GatewayError::Unavailable(format!("{status}"))),
        }
    }

    async fn cancel_nfse(
        &self,
        issuer: &FiscalSettings,
        reference: &str,
        reason: &str,
    ) -> Result<EventOutcome, GatewayError> {
        let (base, token) = self.target(issuer)?;
        let reference = path_reference(reference)?;
        let answer = self
            .call(
                Method::DELETE,
                &format!("{base}/v2/nfsen/{reference}"),
                token,
                Some(json!({ "justificativa": reason }).to_string()),
            )
            .await?;
        self.event_of(
            &base,
            token,
            &answer,
            "cancelado",
            &["caminho_xml_cancelamento"],
            refusal_of_nfse,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::{Path, Query, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use base64_header::basic;
    use conta_verde_domain::{CalendarDate, state_code};

    use super::*;
    use crate::contract::{self, fiscal_issuer, nfe_request};

    const HOMOLOGATION_TOKEN: &str = "token-of-homologation";
    const PRODUCTION_TOKEN: &str = "token-of-production";
    const CNPJ: &str = "11222333000181";

    /// The `Authorization` header of HTTP Basic with this token and an empty password.
    mod base64_header {
        pub fn basic(token: &str) -> String {
            const TABLE: &[u8; 64] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let bytes = format!("{token}:").into_bytes();
            let mut out = String::from("Basic ");
            for chunk in bytes.chunks(3) {
                let n = chunk
                    .iter()
                    .enumerate()
                    .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
                for i in 0..4 {
                    if i <= chunk.len() {
                        out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
                    } else {
                        out.push('=');
                    }
                }
            }
            out
        }
    }

    #[derive(Clone)]
    struct Stored {
        stage: &'static str,
        asked: u32,
        number: u32,
        key: String,
        corrections: u32,
    }

    // A local stand-in for the Focus NFe API as its documentation describes it: the tests never reach the real one.
    // It checks the token, assigns numbers per series, answers an issue with 202 and the document after the second
    // question, and keeps the bodies it was sent.
    #[derive(Default)]
    struct Fake {
        documents: HashMap<String, Stored>,
        last_number: HashMap<String, u32>,
        bodies: Vec<String>,
        token_seen: Vec<String>,
        schema_error: bool,
        unknown_status: bool,
        server_error: bool,
        nfse: HashMap<String, Stored>,
        /// The DPS (series, number) each issuer used, and under which reference.
        dps_used: HashMap<(String, u64, u64), String>,
        nfse_bodies: Vec<String>,
        danfse_on_gateway: bool,
    }

    type Shared = Arc<Mutex<Fake>>;

    fn authorized(headers: &HeaderMap, expect: &str) -> bool {
        headers
            .get("authorization")
            .is_some_and(|value| value == &basic(expect))
    }

    fn json_reply(status: StatusCode, body: Value) -> axum::response::Response {
        (status, axum::Json(body)).into_response()
    }

    fn files(key: &str, number: u32) -> Value {
        json!({
            "chave_nfe": format!("NFe{key}"),
            "numero": number.to_string(),
            "serie": "1",
            "caminho_xml_nota_fiscal": format!("/arquivos/{key}-nfe.xml"),
            "caminho_danfe": format!("/arquivos/{key}-danfe.pdf"),
        })
    }

    fn answer_of(stored: &Stored) -> Value {
        match stored.stage {
            "processando_autorizacao" => json!({ "status": "processando_autorizacao" }),
            "cancelado" => {
                let mut body = files(&stored.key, stored.number);
                body["status"] = json!("cancelado");
                body["caminho_xml_cancelamento"] =
                    json!(format!("/arquivos/{}-cancel.xml", stored.key));
                body
            }
            _ => {
                let mut body = files(&stored.key, stored.number);
                body["status"] = json!("autorizado");
                body
            }
        }
    }

    async fn serve(fake: Shared) -> SocketAddr {
        let router = Router::new()
            .route(
                "/v2/nfe",
                post(
                    |State(fake): State<Shared>,
                     headers: HeaderMap,
                     Query(query): Query<HashMap<String, String>>,
                     body: String| async move {
                        let mut fake = fake.lock().unwrap();
                        fake.token_seen.push(
                            headers
                                .get("authorization")
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or("")
                                .to_owned(),
                        );
                        if !authorized(&headers, HOMOLOGATION_TOKEN) && !authorized(&headers, PRODUCTION_TOKEN) {
                            return json_reply(StatusCode::UNAUTHORIZED, json!({"codigo": "nao_autorizado"}));
                        }
                        if fake.server_error {
                            return json_reply(StatusCode::INTERNAL_SERVER_ERROR, json!({}));
                        }
                        let Some(reference) = query.get("ref").cloned() else {
                            return json_reply(
                                StatusCode::BAD_REQUEST,
                                json!({"codigo": "requisicao_invalida", "mensagem": "ref"}),
                            );
                        };
                        fake.bodies.push(body.clone());
                        let sent: Value = serde_json::from_str(&body).unwrap();
                        if fake.schema_error {
                            return json_reply(
                                StatusCode::UNPROCESSABLE_ENTITY,
                                json!({
                                    "codigo": "erro_validacao_schema",
                                    "mensagem": "Erro na validação do XML",
                                    "erros": [{"mensagem": "valor inválido", "campo": "items.0.codigo_ncm"}]
                                }),
                            );
                        }
                        if let Some(known) = fake.documents.get(&reference) {
                            let code = if known.stage == "processando_autorizacao" {
                                "pending_operation"
                            } else {
                                "already_processed"
                            };
                            return json_reply(
                                StatusCode::UNPROCESSABLE_ENTITY,
                                json!({"codigo": code, "mensagem": "already"}),
                            );
                        }
                        let cnpj = sent["cnpj_emitente"].as_str().unwrap().to_owned();
                        let series = "1".to_owned();
                        let number = fake.last_number.entry(format!("{cnpj}/{series}")).or_insert(0);
                        *number += 1;
                        let number = *number;
                        let key = AccessKey::from_parts(
                            state_code("SP").unwrap(),
                            CalendarDate::parse("2026-11-09").unwrap(),
                            &cnpj,
                            55,
                            1,
                            number,
                            1,
                            10_000_000 + number,
                        )
                        .unwrap()
                        .as_str()
                        .to_owned();
                        fake.documents.insert(
                            reference.clone(),
                            Stored {
                                stage: "processando_autorizacao",
                                asked: 0,
                                number,
                                key,
                                corrections: 0,
                            },
                        );
                        json_reply(
                            StatusCode::ACCEPTED,
                            json!({"cnpj_emitente": cnpj, "ref": reference, "status": "processando_autorizacao"}),
                        )
                    },
                ),
            )
            .route(
                "/v2/nfe/inutilizacao",
                post(|State(fake): State<Shared>, headers: HeaderMap, body: String| async move {
                    if !authorized(&headers, HOMOLOGATION_TOKEN) {
                        return json_reply(StatusCode::UNAUTHORIZED, json!({}));
                    }
                    let sent: Value = serde_json::from_str(&body).unwrap();
                    fake.lock().unwrap().bodies.push(body);
                    let first = sent["numero_inicial"].as_u64().unwrap();
                    let last = sent["numero_final"].as_u64().unwrap();
                    if sent["justificativa"].as_str().unwrap().chars().count() < 15 || first > last {
                        return json_reply(
                            StatusCode::OK,
                            json!({"status": "erro_autorizacao", "status_sefaz": "241", "mensagem_sefaz": "Um número da faixa já foi utilizado"}),
                        );
                    }
                    json_reply(
                        StatusCode::OK,
                        json!({"status": "autorizado", "status_sefaz": "102", "protocolo": "135000000001", "caminho_xml": "/arquivos/inut.xml"}),
                    )
                }),
            )
            .route(
                "/v2/nfe/{reference}",
                get(|State(fake): State<Shared>, headers: HeaderMap, Path(reference): Path<String>| async move {
                    if !authorized(&headers, HOMOLOGATION_TOKEN) {
                        return json_reply(StatusCode::UNAUTHORIZED, json!({}));
                    }
                    if reference == "plain-text" {
                        return (StatusCode::OK, "all good").into_response();
                    }
                    if reference == "other-issuer" {
                        let key = AccessKey::from_parts(
                            35,
                            CalendarDate::parse("2026-11-09").unwrap(),
                            "12ABC34501DE35",
                            55,
                            1,
                            7,
                            1,
                            1,
                        )
                        .unwrap();
                        let mut body = files(key.as_str(), 7);
                        body["status"] = json!("autorizado");
                        return json_reply(StatusCode::OK, body);
                    }
                    if reference == "refused-query" {
                        return json_reply(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            json!({"codigo": "permissao_negada", "mensagem": "CNPJ não autorizado"}),
                        );
                    }
                    let mut fake = fake.lock().unwrap();
                    if fake.unknown_status {
                        return json_reply(StatusCode::OK, json!({"status": "algo_novo"}));
                    }
                    let Some(stored) = fake.documents.get_mut(&reference) else {
                        return json_reply(StatusCode::NOT_FOUND, json!({"codigo": "nao_encontrado"}));
                    };
                    if stored.stage == "processando_autorizacao" {
                        stored.asked += 1;
                        if stored.asked >= 2 {
                            stored.stage = "autorizado";
                        }
                    }
                    json_reply(StatusCode::OK, answer_of(stored))
                })
                .delete(
                    |State(fake): State<Shared>,
                     headers: HeaderMap,
                     Path(reference): Path<String>,
                     body: String| async move {
                        if !authorized(&headers, HOMOLOGATION_TOKEN) {
                            return json_reply(StatusCode::UNAUTHORIZED, json!({}));
                        }
                        let sent: Value = serde_json::from_str(&body).unwrap();
                        let reason = sent["justificativa"].as_str().unwrap_or("");
                        if !(15..=255).contains(&reason.chars().count()) {
                            return json_reply(
                                StatusCode::BAD_REQUEST,
                                json!({"codigo": "requisicao_invalida", "mensagem": "justificativa"}),
                            );
                        }
                        let mut fake = fake.lock().unwrap();
                        match fake.documents.get_mut(&reference) {
                            Some(stored) if stored.stage == "autorizado" => {
                                stored.stage = "cancelado";
                                json_reply(
                                    StatusCode::OK,
                                    json!({"status": "cancelado", "status_sefaz": "135",
                                           "caminho_xml_cancelamento": format!("/arquivos/{}-cancel.xml", stored.key)}),
                                )
                            }
                            Some(_) => json_reply(
                                StatusCode::UNPROCESSABLE_ENTITY,
                                json!({"codigo": "nfe_nao_autorizada", "mensagem": "A nota não está autorizada"}),
                            ),
                            None => json_reply(StatusCode::NOT_FOUND, json!({"codigo": "nao_encontrado"})),
                        }
                    },
                ),
            )
            .route(
                "/v2/nfe/{reference}/carta_correcao",
                post(
                    |State(fake): State<Shared>,
                     headers: HeaderMap,
                     Path(reference): Path<String>,
                     body: String| async move {
                        if !authorized(&headers, HOMOLOGATION_TOKEN) {
                            return json_reply(StatusCode::UNAUTHORIZED, json!({}));
                        }
                        let sent: Value = serde_json::from_str(&body).unwrap();
                        let text = sent["correcao"].as_str().unwrap_or("");
                        if !(15..=1000).contains(&text.chars().count()) {
                            return json_reply(
                                StatusCode::BAD_REQUEST,
                                json!({"codigo": "requisicao_invalida", "mensagem": "correcao"}),
                            );
                        }
                        let mut fake = fake.lock().unwrap();
                        match fake.documents.get_mut(&reference) {
                            Some(stored) if stored.stage == "autorizado" => {
                                stored.corrections += 1;
                                json_reply(
                                    StatusCode::OK,
                                    json!({"status": "autorizado", "status_sefaz": "135",
                                           "numero_carta_correcao": stored.corrections,
                                           "caminho_xml_carta_correcao": format!("/arquivos/{}-cce.xml", stored.key),
                                           "caminho_pdf_carta_correcao": format!("/arquivos/{}-cce.pdf", stored.key)}),
                                )
                            }
                            _ => json_reply(
                                StatusCode::BAD_REQUEST,
                                json!({"codigo": "requisicao_invalida", "mensagem": "A nota não está autorizada"}),
                            ),
                        }
                    },
                ),
            )
            .route(
                "/v2/nfsen",
                post(
                    |State(fake): State<Shared>,
                     headers: HeaderMap,
                     Query(query): Query<HashMap<String, String>>,
                     body: String| async move {
                        if !authorized(&headers, HOMOLOGATION_TOKEN) {
                            return json_reply(StatusCode::UNAUTHORIZED, json!({"codigo": "nao_autorizado"}));
                        }
                        let mut fake = fake.lock().unwrap();
                        let Some(reference) = query.get("ref").cloned() else {
                            return json_reply(
                                StatusCode::BAD_REQUEST,
                                json!({"codigo": "requisicao_invalida", "mensagem": "ref"}),
                            );
                        };
                        fake.nfse_bodies.push(body.clone());
                        let sent: Value = serde_json::from_str(&body).unwrap();
                        if fake.nfse.contains_key(&reference) {
                            return json_reply(
                                StatusCode::UNPROCESSABLE_ENTITY,
                                json!({"codigo": "erro_validacao", "mensagem": "referência duplicada"}),
                            );
                        }
                        let cnpj = sent["cnpj_prestador"].as_str().unwrap().to_owned();
                        let dps = (
                            cnpj.clone(),
                            sent["serie_dps"].as_u64().unwrap(),
                            sent["numero_dps"].as_u64().unwrap(),
                        );
                        if fake.dps_used.contains_key(&dps) {
                            return json_reply(
                                StatusCode::UNPROCESSABLE_ENTITY,
                                json!({"codigo": "erro_validacao", "mensagem": "DPS já recebida"}),
                            );
                        }
                        fake.dps_used.insert(dps, reference.clone());
                        let number = fake.nfse.len() as u32 + 1;
                        fake.nfse.insert(
                            reference.clone(),
                            Stored {
                                stage: "processando_autorizacao",
                                asked: 0,
                                number,
                                key: format!("nfse{number}"),
                                corrections: 0,
                            },
                        );
                        json_reply(
                            StatusCode::ACCEPTED,
                            json!({"status": "processando_autorizacao", "ref": reference, "protocolo": "P1"}),
                        )
                    },
                ),
            )
            .route(
                "/v2/nfsen/{reference}",
                get(|State(fake): State<Shared>, headers: HeaderMap, Path(reference): Path<String>| async move {
                    if !authorized(&headers, HOMOLOGATION_TOKEN) {
                        return json_reply(StatusCode::UNAUTHORIZED, json!({}));
                    }
                    let mut fake = fake.lock().unwrap();
                    let on_gateway = fake.danfse_on_gateway;
                    let Some(stored) = fake.nfse.get_mut(&reference) else {
                        return json_reply(StatusCode::NOT_FOUND, json!({"codigo": "nao_encontrado"}));
                    };
                    if stored.stage == "processando_autorizacao" {
                        stored.asked += 1;
                        if stored.asked >= 2 {
                            stored.stage = "autorizado";
                        }
                    }
                    if stored.stage == "processando_autorizacao" {
                        return json_reply(StatusCode::OK, json!({"status": "processando_autorizacao", "ref": reference}));
                    }
                    let mut body = json!({
                        "status": stored.stage,
                        "numero": stored.number.to_string(),
                        "codigo_verificacao": format!("V{}", stored.number),
                        "caminho_xml_nota_fiscal": format!("/arquivos/{}-nfse.xml", stored.key),
                        "url_danfse": if on_gateway {
                            format!("/arquivos/{}-danfse.pdf", stored.key)
                        } else {
                            "https://storage.example.com/danfse.pdf".to_owned()
                        },
                    });
                    if stored.stage == "cancelado" {
                        body["caminho_xml_cancelamento"] = json!(format!("/arquivos/{}-cancel.xml", stored.key));
                    }
                    json_reply(StatusCode::OK, body)
                })
                .delete(
                    |State(fake): State<Shared>,
                     headers: HeaderMap,
                     Path(reference): Path<String>,
                     body: String| async move {
                        if !authorized(&headers, HOMOLOGATION_TOKEN) {
                            return json_reply(StatusCode::UNAUTHORIZED, json!({}));
                        }
                        let sent: Value = serde_json::from_str(&body).unwrap();
                        let reason = sent["justificativa"].as_str().unwrap_or("");
                        let mut fake = fake.lock().unwrap();
                        match fake.nfse.get_mut(&reference) {
                            Some(stored) if stored.stage == "autorizado" && reason.chars().count() >= 15 => {
                                stored.stage = "cancelado";
                                json_reply(StatusCode::OK, json!({"status": "cancelado"}))
                            }
                            Some(_) => json_reply(
                                StatusCode::OK,
                                json!({"status": "erro_cancelamento", "erros": [
                                    {"codigo": "V999", "mensagem": "NFSe fora do prazo de cancelamento permitido", "correcao": null}
                                ]}),
                            ),
                            None => json_reply(StatusCode::NOT_FOUND, json!({"codigo": "nao_encontrado"})),
                        }
                    },
                ),
            )
            .route(
                "/arquivos/{name}",
                get(|headers: HeaderMap, Path(name): Path<String>| async move {
                    if !authorized(&headers, HOMOLOGATION_TOKEN) {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                    let key = name.split('-').next().unwrap_or("").to_owned();
                    if name.ends_with(".pdf") {
                        return (StatusCode::OK, b"%PDF-1.4 stand-in".to_vec()).into_response();
                    }
                    (StatusCode::OK, format!("<xml><!-- {key} --></xml>").into_bytes()).into_response()
                }),
            )
            .with_state(fake);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        addr
    }

    fn gateway(
        addr: SocketAddr,
        homologation: Option<&str>,
        production: Option<&str>,
    ) -> FocusNfeGateway {
        FocusNfeGateway::with_timeout(
            Some(&format!("http://{addr}/")),
            homologation,
            production,
            CNPJ,
            Duration::from_secs(2),
        )
        .unwrap()
    }

    fn homologation() -> FiscalSettings {
        fiscal_issuer(CNPJ, FiscalEnvironment::Homologation)
    }

    #[tokio::test]
    async fn authorizes_cancels_and_corrects_through_the_stand_in() {
        let addr = serve(Shared::default()).await;
        contract::a_gateway_authorizes_cancels_and_corrects(
            &gateway(addr, Some(HOMOLOGATION_TOKEN), None),
            &homologation(),
        )
        .await;
    }

    #[tokio::test]
    async fn the_token_of_the_environment_is_the_one_sent_and_a_missing_one_makes_it_unavailable() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let both = gateway(addr, Some(HOMOLOGATION_TOKEN), Some(PRODUCTION_TOKEN));
        both.submit_nfe(&nfe_request(&homologation(), "a1"))
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        both.submit_nfe(&nfe_request(
            &fiscal_issuer(CNPJ, FiscalEnvironment::Production),
            "a2",
        ))
        .await
        .map(|submission| submission.outcome)
        .unwrap();
        assert_eq!(
            fake.lock().unwrap().token_seen,
            [basic(HOMOLOGATION_TOKEN), basic(PRODUCTION_TOKEN)]
        );
        // A gateway with only the homologation token cannot issue in production, and says why without a call.
        let only_homologation = gateway(addr, Some(HOMOLOGATION_TOKEN), None);
        let error = only_homologation
            .submit_nfe(&nfe_request(
                &fiscal_issuer(CNPJ, FiscalEnvironment::Production),
                "a3",
            ))
            .await
            .map(|submission| submission.outcome)
            .unwrap_err();
        assert!(
            matches!(error, GatewayError::Unavailable(reason) if reason.contains("production"))
        );
        assert_eq!(fake.lock().unwrap().bodies.len(), 2);
    }

    #[tokio::test]
    async fn a_document_never_goes_out_under_another_companys_token() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let other = fiscal_issuer(
            contract::CLOSED_ALPHANUMERIC_CNPJ,
            FiscalEnvironment::Homologation,
        );
        let gateway = gateway(addr, Some(HOMOLOGATION_TOKEN), None);
        assert_eq!(
            gateway
                .submit_nfe(&nfe_request(&other, "b1"))
                .await
                .map(|submission| submission.outcome),
            Err(GatewayError::WrongIssuer)
        );
        assert_eq!(
            gateway.query_nfe(&other, "b1").await,
            Err(GatewayError::WrongIssuer)
        );
        assert_eq!(
            gateway
                .cancel_nfe(&other, "b1", "Venda desfeita a pedido do cliente")
                .await,
            Err(GatewayError::WrongIssuer)
        );
        assert!(fake.lock().unwrap().bodies.is_empty());
    }

    #[tokio::test]
    async fn amounts_travel_as_the_exact_decimals_they_are_and_text_cannot_pass_for_one() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let mut request = nfe_request(&homologation(), "c1");
        request.items[0].description = "Item \"@@\" com @@ e \\ barras".into();
        request.items[0].discount = Amount::parse("0.01").unwrap();
        gateway(addr, Some(HOMOLOGATION_TOKEN), None)
            .submit_nfe(&request)
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        let body = fake.lock().unwrap().bodies[0].clone();
        for fragment in [
            "\"quantidade_comercial\":100,",
            "\"valor_unitario_comercial\":0.4723,",
            "\"valor_bruto\":47.23,",
            "\"valor_desconto\":0.01",
            "\"valor_total\":47.23",
            "\"data_emissao\":\"2026-11-09T14:30:00-03:00\"",
            "\"regime_tributario_emitente\":1",
            "\"cpf_destinatario\":\"52998224725\"",
            "\"forma_pagamento\":\"17\"",
        ] {
            assert!(body.contains(fragment), "{fragment} in {body}");
        }
        assert!(!body.contains("e-") && !body.contains("E+"), "{body}");
        let sent: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            sent["items"][0]["descricao"],
            "Item \"@@\" com @@ e \\ barras"
        );
        // An item without a discount sends none.
        let mut plain = nfe_request(&homologation(), "c2");
        plain.items[0].discount = Amount::parse("0").unwrap();
        gateway(addr, Some(HOMOLOGATION_TOKEN), None)
            .submit_nfe(&plain)
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        let plain_body: Value = serde_json::from_str(&fake.lock().unwrap().bodies[1]).unwrap();
        assert!(plain_body["items"][0].get("valor_desconto").is_none());
    }

    #[tokio::test]
    async fn a_document_the_gateway_refuses_is_a_refusal_the_person_can_read() {
        let fake = Shared::default();
        fake.lock().unwrap().schema_error = true;
        let addr = serve(fake).await;
        let outcome = gateway(addr, Some(HOMOLOGATION_TOKEN), None)
            .submit_nfe(&nfe_request(&homologation(), "d1"))
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        match outcome {
            DocumentOutcome::Rejected(refusal) => {
                assert_eq!(refusal.code, "erro_validacao_schema");
                assert!(
                    refusal.message.contains("items.0.codigo_ncm"),
                    "{}",
                    refusal.message
                );
            }
            other => panic!("rejected was expected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn asking_again_about_a_reference_the_gateway_knows_gives_its_state_not_an_error() {
        let addr = serve(Shared::default()).await;
        let gateway = gateway(addr, Some(HOMOLOGATION_TOKEN), None);
        let request = nfe_request(&homologation(), "e1");
        assert_eq!(
            gateway
                .submit_nfe(&request)
                .await
                .map(|submission| submission.outcome)
                .unwrap(),
            DocumentOutcome::Processing
        );
        // `pending_operation` while it is processing, `already_processed` once it is not: both mean "query".
        let second = gateway
            .submit_nfe(&request)
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        assert!(matches!(
            second,
            DocumentOutcome::Processing | DocumentOutcome::Authorized(_)
        ));
        let third = gateway
            .submit_nfe(&request)
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        assert!(matches!(third, DocumentOutcome::Authorized(_)), "{third:?}");
        let fourth = gateway
            .submit_nfe(&request)
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        assert_eq!(third, fourth);
    }

    #[tokio::test]
    async fn a_failure_of_the_gateway_is_unavailable_and_never_carries_a_body() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let request = nfe_request(&homologation(), "f1");
        // The token being refused is not the document's fault.
        let wrong = gateway(addr, Some("another-token"), None);
        assert!(matches!(
            wrong.submit_nfe(&request).await.map(|submission| submission.outcome),
            Err(GatewayError::Unavailable(reason)) if reason == "401 Unauthorized"
        ));
        fake.lock().unwrap().server_error = true;
        assert!(matches!(
            gateway(addr, Some(HOMOLOGATION_TOKEN), None).submit_nfe(&request).await.map(|submission| submission.outcome),
            Err(GatewayError::Unavailable(reason)) if reason.starts_with("500")
        ));
        // A status this adapter does not know is not guessed at.
        let fake = Shared::default();
        fake.lock().unwrap().unknown_status = true;
        let addr = serve(fake).await;
        assert!(matches!(
            gateway(addr, Some(HOMOLOGATION_TOKEN), None)
                .query_nfe(&homologation(), "f2")
                .await,
            Err(GatewayError::Unavailable(reason)) if reason.contains("algo_novo")
        ));
        // An answer that is not JSON, and a document of another issuer's key, are not taken.
        let addr = serve(Shared::default()).await;
        let focus = gateway(addr, Some(HOMOLOGATION_TOKEN), None);
        assert!(matches!(
            focus.query_nfe(&homologation(), "plain-text").await,
            Err(GatewayError::Unavailable(reason)) if reason.contains("not JSON")
        ));
        assert!(matches!(
            focus.query_nfe(&homologation(), "other-issuer").await,
            Err(GatewayError::Unavailable(reason)) if reason.contains("does not match")
        ));
        // A question the gateway refuses is no refusal of the document.
        let addr = serve(Shared::default()).await;
        assert!(matches!(
            gateway(addr, Some(HOMOLOGATION_TOKEN), None)
                .query_nfe(&homologation(), "refused-query")
                .await,
            Err(GatewayError::Unavailable(reason)) if reason.starts_with("422")
        ));
        // And nothing listens: the address is not in the text.
        let dead = gateway(
            "127.0.0.1:1".parse().unwrap(),
            Some(HOMOLOGATION_TOKEN),
            None,
        );
        match dead.query_nfe(&homologation(), "f3").await {
            Err(GatewayError::Unavailable(reason)) => assert!(!reason.contains("f3"), "{reason}"),
            other => panic!("unavailable was expected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_regular_regimes_wait_for_the_tax_calculation_and_a_reference_is_a_plain_id() {
        let addr = serve(Shared::default()).await;
        let gateway = gateway(addr, Some(HOMOLOGATION_TOKEN), None);
        let mut draft = contract::fiscal_settings_draft(CNPJ, FiscalEnvironment::Homologation);
        draft.regime = TaxRegime::LucroPresumido;
        let presumed = FiscalSettings::new(homologation().company_id(), draft).unwrap();
        assert!(matches!(
            gateway
                .submit_nfe(&nfe_request(&presumed, "g1"))
                .await
                .map(|submission| submission.outcome),
            Err(GatewayError::Unsupported(_))
        ));
        assert!(matches!(
            gateway.query_nfe(&homologation(), "../../v2/other").await,
            Err(GatewayError::Unsupported(_))
        ));
    }

    /// Homologation, run by hand against the gateway's test environment with `FOCUS_NFE_SANDBOX_TOKEN` and
    /// `FOCUS_NFE_SANDBOX_CNPJ` set (the company registered at the gateway, **with a real A1 certificate even in
    /// homologation**). CI runs the ignored tests without them and this one does nothing. It issues one NF-e from a
    /// sale of goods and asks for it, which settles what this adapter's documentation reading left open: the fields,
    /// contingency, and where the files are. Fill in the issuer's data of that company before running.
    #[tokio::test]
    async fn an_nfse_is_authorized_and_cancelled_through_the_stand_in() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let issuer = contract::fiscal_service_issuer(CNPJ, FiscalEnvironment::Homologation);
        contract::a_gateway_authorizes_and_cancels_an_nfse(
            &gateway(addr, Some(HOMOLOGATION_TOKEN), None),
            &issuer,
        )
        .await;
    }

    #[tokio::test]
    async fn the_dps_goes_out_in_the_fields_of_the_national_layout_with_exact_amounts() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let issuer = contract::fiscal_service_issuer(CNPJ, FiscalEnvironment::Homologation);
        let mut request = contract::nfse_request(&issuer, "dps-ref", 42);
        request.gross = Amount::parse("1234567.89").unwrap();
        request.discount = Amount::parse("0.10").unwrap();
        gateway(addr, Some(HOMOLOGATION_TOKEN), None)
            .submit_nfse(&request)
            .await
            .unwrap();
        let sent: Value = serde_json::from_str(&fake.lock().unwrap().nfse_bodies[0]).unwrap();
        assert_eq!(sent["numero_dps"], 42);
        assert_eq!(sent["serie_dps"], 1);
        assert_eq!(sent["data_competencia"], "2026-11-09");
        assert_eq!(sent["emitente_dps"], 1);
        assert_eq!(sent["codigo_municipio_emissora"], 3_550_308);
        assert_eq!(sent["codigo_municipio_prestacao"], "3550308");
        assert_eq!(sent["codigo_opcao_simples_nacional"], 3);
        assert_eq!(sent["regime_especial_tributacao"], 0);
        assert_eq!(sent["codigo_tributacao_nacional_iss"], "171001");
        assert_eq!(sent["codigo_tributacao_municipal_iss"], "1701");
        assert_eq!(sent["tributacao_iss"], 1);
        assert_eq!(sent["cpf_tomador"], "52998224725");
        assert!(sent.get("cnpj_tomador").is_none());
        // Amounts are the exact decimals, never a float that would round them.
        let raw = fake.lock().unwrap().nfse_bodies[0].clone();
        assert!(raw.contains("\"valor_servico\":1234567.89"), "{raw}");
        assert!(
            raw.contains("\"desconto_incondicionado\":0.10")
                || raw.contains("\"desconto_incondicionado\":0.1"),
            "{raw}"
        );
        assert!(
            raw.contains("\"percentual_aliquota_relativa_municipio\":2.5"),
            "{raw}"
        );
    }

    #[tokio::test]
    async fn a_duplicate_reference_is_the_document_it_made_and_a_refusal_is_one_the_person_can_read()
     {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let issuer = contract::fiscal_service_issuer(CNPJ, FiscalEnvironment::Homologation);
        let gateway = gateway(addr, Some(HOMOLOGATION_TOKEN), None);
        let first = gateway
            .submit_nfse(&contract::nfse_request(&issuer, "dup", 1))
            .await
            .unwrap();
        assert_eq!(first.outcome, DocumentOutcome::Processing);
        // The same reference again: the gateway says "duplicate" (422), and asking tells it is the one it holds.
        let again = gateway
            .submit_nfse(&contract::nfse_request(&issuer, "dup", 1))
            .await
            .unwrap();
        assert_eq!(again.outcome, DocumentOutcome::Processing);
        // Another reference with the same DPS number: nothing of that reference exists, so it is a refusal.
        let refused = gateway
            .submit_nfse(&contract::nfse_request(&issuer, "other", 1))
            .await
            .unwrap();
        assert!(matches!(
            refused.outcome,
            DocumentOutcome::Rejected(refusal) if refusal.code == "erro_validacao"
        ));
    }

    #[tokio::test]
    async fn the_xml_is_required_and_the_pdf_is_kept_only_when_the_gateway_hosts_it() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let issuer = contract::fiscal_service_issuer(CNPJ, FiscalEnvironment::Homologation);
        let gateway = gateway(addr, Some(HOMOLOGATION_TOKEN), None);
        let authorize = |reference: &'static str, dps: u64| {
            let gateway = gateway.clone();
            let issuer = issuer.clone();
            async move {
                gateway
                    .submit_nfse(&contract::nfse_request(&issuer, reference, dps))
                    .await
                    .unwrap();
                gateway.query_nfse(&issuer, reference).await.unwrap();
                match gateway.query_nfse(&issuer, reference).await.unwrap() {
                    DocumentOutcome::Authorized(document) => document,
                    other => panic!("authorized was expected, got {other:?}"),
                }
            }
        };
        let elsewhere = authorize("pdf-elsewhere", 1).await;
        assert!(!elsewhere.xml.is_empty());
        assert_eq!(
            elsewhere.pdf, None,
            "a file on another host is never followed"
        );
        assert_eq!(elsewhere.verification_code.as_deref(), Some("V1"));
        assert_eq!(elsewhere.key, None);
        fake.lock().unwrap().danfse_on_gateway = true;
        let hosted = authorize("pdf-hosted", 2).await;
        assert!(hosted.pdf.is_some());
    }

    #[tokio::test]
    async fn an_nfse_the_gateway_cannot_describe_is_unavailable_and_the_other_regimes_wait() {
        let fake = Shared::default();
        let addr = serve(fake.clone()).await;
        let gateway = gateway(addr, Some(HOMOLOGATION_TOKEN), None);
        // An issuer without the municipality code cannot be told to the national system.
        let mut without_city =
            contract::fiscal_settings_draft(CNPJ, FiscalEnvironment::Homologation);
        without_city.city_ibge_code = None;
        let without_city =
            FiscalSettings::new(conta_verde_domain::CompanyId::generate(), without_city).unwrap();
        assert!(matches!(
            gateway
                .submit_nfse(&contract::nfse_request(&without_city, "x", 1))
                .await,
            Err(GatewayError::Unsupported(reason)) if reason.contains("municipality")
        ));
        // A document of another company's token is never sent.
        let other =
            contract::fiscal_service_issuer("11444777000161", FiscalEnvironment::Homologation);
        assert_eq!(
            gateway
                .submit_nfse(&contract::nfse_request(&other, "y", 1))
                .await
                .unwrap_err(),
            GatewayError::WrongIssuer
        );
        assert!(fake.lock().unwrap().nfse_bodies.is_empty());
    }

    #[tokio::test]
    #[ignore = "needs FOCUS_NFE_SANDBOX_TOKEN, FOCUS_NFE_SANDBOX_CNPJ, a real certificate and the network"]
    async fn the_sandbox_authorizes_an_nfe() {
        let (Ok(token), Ok(cnpj)) = (
            std::env::var("FOCUS_NFE_SANDBOX_TOKEN"),
            std::env::var("FOCUS_NFE_SANDBOX_CNPJ"),
        ) else {
            return;
        };
        let gateway = FocusNfeGateway::new(None, Some(&token), None, &cnpj).unwrap();
        let issuer = fiscal_issuer(&cnpj, FiscalEnvironment::Homologation);
        let reference = format!("sandbox-{}", uuid::Uuid::new_v4().simple());
        let mut request = nfe_request(&issuer, &reference);
        // The sandbox refuses a moment far from its clock.
        request.issued_at_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let first = gateway
            .submit_nfe(&request)
            .await
            .map(|submission| submission.outcome)
            .unwrap();
        println!("submitted: {first:?}");
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let outcome = gateway.query_nfe(&issuer, &reference).await.unwrap();
            println!("queried: {outcome:?}");
            if !matches!(outcome, DocumentOutcome::Processing) {
                assert!(
                    matches!(outcome, DocumentOutcome::Authorized(_)),
                    "{outcome:?}"
                );
                return;
            }
        }
        panic!("the document never left processing");
    }
}
