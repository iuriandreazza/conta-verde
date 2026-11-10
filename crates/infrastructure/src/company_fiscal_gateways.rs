use std::sync::Arc;

use conta_verde_application::{
    ApplicationError, FiscalGatewayResolver, GatewayError, GatewaySettingsRepository,
    GatewaySettingsService, SecretCipher,
};
use conta_verde_domain::{
    CompanyId, FiscalEnvironment, FiscalSettings, GatewayKind, GatewaySecret, GatewaySettings,
};

use crate::{CityCodes, ConfiguredFiscalGateway, FocusNfeGateway, NfeIoGateway};

/// What only the operator decides about how a gateway is reached, whoever's token it is ([ADR 0085]): the address, the hosts a file
/// may be downloaded from, the city codes and the confirmation of the NFE.io search. **A company's settings never carry these**: a
/// company that could name the address its token goes to could send it anywhere.
#[derive(Clone)]
pub struct GatewayEndpoints {
    pub focus_url: Option<String>,
    pub nfeio_url: Option<String>,
    pub nfeio_file_hosts: Vec<String>,
    /// `NFEIO_SEARCH_CONFIRMED`: a production issue through NFE.io needs it **and** the company's own flag, because what it
    /// confirms is what keeps a document from being issued twice ([ADR 0079]).
    pub nfeio_search_confirmed: bool,
    pub cities: Arc<dyn CityCodes>,
}

/// Finds the gateway of a company at the moment of each call ([ADR 0085]): the one built from **its own settings** when it has
/// set them, the server's otherwise. The settings are read again on every call, so a change or a reset is felt at once and a
/// token is never kept past the call that needed it.
///
/// A company with its own settings that are not enough for the environment of the call is refused, naming what is missing: the
/// server's gateway is never put in its place (it would issue under another token), and the settings are never half used.
pub struct CompanyFiscalGateways<R, C> {
    server: ConfiguredFiscalGateway,
    settings: GatewaySettingsService<R, C>,
    endpoints: GatewayEndpoints,
}

impl<R: GatewaySettingsRepository, C: SecretCipher> CompanyFiscalGateways<R, C> {
    pub fn new(
        server: ConfiguredFiscalGateway,
        settings: GatewaySettingsService<R, C>,
        endpoints: GatewayEndpoints,
    ) -> Self {
        Self {
            server,
            settings,
            endpoints,
        }
    }

    fn build(
        &self,
        row: &GatewaySettings,
        issuer: &FiscalSettings,
    ) -> Result<ConfiguredFiscalGateway, ApplicationError> {
        let production = issuer.environment() == FiscalEnvironment::Production;
        let environment = if production {
            "production"
        } else {
            "homologation"
        };
        let missing = |what: &str| {
            ApplicationError::FiscalRule(format!(
                "the gateway settings have no {what} for {environment}: set it in the gateway settings, or reset them to use the server's gateway"
            ))
        };
        let unavailable =
            |error: reqwest::Error| GatewayError::Unavailable(error.without_url().to_string());
        match row.kind() {
            GatewayKind::Focus => {
                let token = self
                    .settings
                    .open_from(
                        row,
                        if production {
                            GatewaySecret::FocusTokenProduction
                        } else {
                            GatewaySecret::FocusTokenHomologation
                        },
                    )?
                    .ok_or_else(|| missing("Focus NFe token"))?;
                let (homologation, production) = if production {
                    (None, Some(token.expose()))
                } else {
                    (Some(token.expose()), None)
                };
                let gateway = FocusNfeGateway::new(
                    self.endpoints.focus_url.as_deref(),
                    homologation,
                    production,
                    issuer.cnpj(),
                )
                .map_err(unavailable)?;
                Ok(ConfiguredFiscalGateway::Focus(Box::new(gateway)))
            }
            GatewayKind::NfeIo => {
                let key = self
                    .settings
                    .open_from(row, GatewaySecret::NfeIoApiKey)?
                    .ok_or_else(|| missing("NFE.io API key"))?;
                let company = if production {
                    row.nfeio_company_production()
                } else {
                    row.nfeio_company_homologation()
                }
                .ok_or_else(|| missing("NFE.io company id"))?;
                let (homologation, production) = if production {
                    (None, Some(company))
                } else {
                    (Some(company), None)
                };
                let gateway = NfeIoGateway::new(
                    self.endpoints.nfeio_url.as_deref(),
                    key.expose(),
                    homologation,
                    production,
                    issuer.cnpj(),
                    self.endpoints.cities.clone(),
                )
                .map_err(unavailable)?
                .with_file_hosts(&self.endpoints.nfeio_file_hosts)
                .with_search_confirmed(
                    row.nfeio_search_confirmed() && self.endpoints.nfeio_search_confirmed,
                );
                Ok(ConfiguredFiscalGateway::NfeIo(Box::new(gateway)))
            }
        }
    }
}

impl<R: GatewaySettingsRepository, C: SecretCipher> FiscalGatewayResolver
    for CompanyFiscalGateways<R, C>
{
    type Gateway = ConfiguredFiscalGateway;

    async fn resolve(
        &self,
        issuer: &FiscalSettings,
    ) -> Result<ConfiguredFiscalGateway, ApplicationError> {
        match self.settings.get(issuer.company_id()).await? {
            Some(row) => self.build(&row, issuer),
            None => Ok(self.server.clone()),
        }
    }

    fn server_key(&self) -> Option<String> {
        conta_verde_application::FiscalGateway::is_enabled(&self.server)
            .then(|| conta_verde_application::FiscalGateway::key(&self.server).to_owned())
    }

    async fn key_for(&self, company_id: CompanyId) -> Option<String> {
        match self.settings.get(company_id).await {
            Ok(Some(row)) => Some(row.kind().key().to_owned()),
            Ok(None) => self.server_key(),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use conta_verde_application::{FiscalGateway, GatewaySettingsInput, SecretChange};
    use conta_verde_domain::{Document, FiscalEnvironment, SecretText};
    use serde_json::json;

    use super::*;
    use crate::contract::{self, SECRETS_KEY, fiscal_settings_draft, nfe_request};
    use crate::focus_nfe::tests::base64_header::basic;
    use crate::{
        InMemoryGatewaySettingsRepository, NoSecretCipher, RingSecretCipher, StaticCityCodes,
    };

    const SERVER_TOKEN: &str = "token-of-the-server-gateway";
    const SERVER_CNPJ: &str = "11222333000181";
    const OWN_CNPJ: &str = "11444777000161";

    type Seen = Arc<Mutex<Vec<String>>>;

    /// A stand-in for a gateway that only says what token each call came with: the tests never reach a real one.
    async fn standin() -> (String, Seen) {
        let seen = Seen::default();
        let router = Router::new()
            .fallback(|State(seen): State<Seen>, headers: HeaderMap| async move {
                seen.lock().unwrap().push(
                    headers
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("")
                        .to_owned(),
                );
                (
                    StatusCode::OK,
                    axum::Json(json!({"status": "processando_autorizacao"})),
                )
                    .into_response()
            })
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (format!("http://{address}"), seen)
    }

    fn issuer(
        company: CompanyId,
        cnpj: &str,
        environment: FiscalEnvironment,
    ) -> conta_verde_domain::FiscalSettings {
        conta_verde_domain::FiscalSettings::new(company, fiscal_settings_draft(cnpj, environment))
            .unwrap()
    }

    fn endpoints(url: Option<String>, search_confirmed: bool) -> GatewayEndpoints {
        GatewayEndpoints {
            focus_url: url.clone(),
            nfeio_url: url,
            nfeio_file_hosts: Vec::new(),
            nfeio_search_confirmed: search_confirmed,
            cities: Arc::new(StaticCityCodes::default()),
        }
    }

    fn server_gateway(url: &str) -> ConfiguredFiscalGateway {
        let cnpj = match Document::parse(SERVER_CNPJ).unwrap() {
            Document::Cnpj(cnpj) => cnpj,
            _ => unreachable!(),
        };
        ConfiguredFiscalGateway::Focus(Box::new(
            FocusNfeGateway::new(Some(url), Some(SERVER_TOKEN), Some(SERVER_TOKEN), &cnpj).unwrap(),
        ))
    }

    fn input(kind: GatewayKind) -> GatewaySettingsInput {
        GatewaySettingsInput {
            kind,
            focus_token_homologation: SecretChange::Keep,
            focus_token_production: SecretChange::Keep,
            nfeio_api_key: SecretChange::Keep,
            webhook_secret: SecretChange::Keep,
            nfeio_company_homologation: None,
            nfeio_company_production: None,
            nfeio_search_confirmed: false,
        }
    }

    fn secret(text: &str) -> SecretChange {
        SecretChange::Set(SecretText::new(text).unwrap())
    }

    struct Setup {
        resolver: CompanyFiscalGateways<InMemoryGatewaySettingsRepository, RingSecretCipher>,
        settings: GatewaySettingsService<InMemoryGatewaySettingsRepository, RingSecretCipher>,
        repository: InMemoryGatewaySettingsRepository,
    }

    fn setup(server: ConfiguredFiscalGateway, endpoints: GatewayEndpoints) -> Setup {
        let repository = InMemoryGatewaySettingsRepository::default();
        let cipher = || RingSecretCipher::from_hex(SECRETS_KEY).unwrap();
        Setup {
            resolver: CompanyFiscalGateways::new(
                server,
                GatewaySettingsService::new(repository.clone(), cipher()),
                endpoints,
            ),
            settings: GatewaySettingsService::new(repository.clone(), cipher()),
            repository,
        }
    }

    /// One company's own token never goes out in another company's call, whichever of them asks first, and the server's token
    /// goes only for the company that has set nothing.
    #[tokio::test]
    async fn a_company_s_token_goes_only_in_its_own_calls() {
        let (url, seen) = standin().await;
        let setup = setup(server_gateway(&url), endpoints(Some(url.clone()), false));
        let (own, plain) = (CompanyId::generate(), CompanyId::generate());
        setup
            .settings
            .replace(
                own,
                &contract::actor(),
                GatewaySettingsInput {
                    focus_token_homologation: secret("token-of-the-company-own"),
                    ..input(GatewayKind::Focus)
                },
            )
            .await
            .unwrap();
        let (for_own, for_plain) = (
            issuer(own, OWN_CNPJ, FiscalEnvironment::Homologation),
            issuer(plain, SERVER_CNPJ, FiscalEnvironment::Homologation),
        );

        for _ in 0..2 {
            let gateway = setup.resolver.resolve(&for_own).await.unwrap();
            gateway.query_nfe(&for_own, "ref-own").await.unwrap();
            let gateway = setup.resolver.resolve(&for_plain).await.unwrap();
            gateway.query_nfe(&for_plain, "ref-plain").await.unwrap();
        }

        assert_eq!(
            *seen.lock().unwrap(),
            [
                basic("token-of-the-company-own"),
                basic(SERVER_TOKEN),
                basic("token-of-the-company-own"),
                basic(SERVER_TOKEN),
            ]
        );
        // The server's gateway is for the CNPJ it was configured for: the company that set nothing and issues under another one
        // is refused before anything is sent, so it can never borrow the server's token.
        let borrowing = issuer(plain, OWN_CNPJ, FiscalEnvironment::Homologation);
        let gateway = setup.resolver.resolve(&borrowing).await.unwrap();
        assert_eq!(
            gateway.query_nfe(&borrowing, "ref").await.unwrap_err(),
            GatewayError::WrongIssuer
        );
        assert_eq!(seen.lock().unwrap().len(), 4);
    }

    /// A change or a reset is felt on the next call: nothing is kept from the last one.
    #[tokio::test]
    async fn a_change_or_a_reset_is_felt_on_the_next_call() {
        let (url, seen) = standin().await;
        let setup = setup(server_gateway(&url), endpoints(Some(url.clone()), false));
        let company = CompanyId::generate();
        let issuer = issuer(company, OWN_CNPJ, FiscalEnvironment::Homologation);
        let user = contract::actor();
        for token in ["token-number-one-0001", "token-number-two-0002"] {
            setup
                .settings
                .replace(
                    company,
                    &user,
                    GatewaySettingsInput {
                        focus_token_homologation: secret(token),
                        ..input(GatewayKind::Focus)
                    },
                )
                .await
                .unwrap();
            let gateway = setup.resolver.resolve(&issuer).await.unwrap();
            gateway.query_nfe(&issuer, "ref").await.unwrap();
        }
        setup.settings.reset(company, &user).await.unwrap();
        // Back to the server's gateway, which is for another CNPJ.
        let gateway = setup.resolver.resolve(&issuer).await.unwrap();
        assert_eq!(
            gateway.query_nfe(&issuer, "ref").await.unwrap_err(),
            GatewayError::WrongIssuer
        );
        assert_eq!(
            *seen.lock().unwrap(),
            [
                basic("token-number-one-0001"),
                basic("token-number-two-0002")
            ]
        );
    }

    /// Settings that do not have what the environment needs are refused, naming what is missing, and the server's gateway is
    /// never put in their place.
    #[tokio::test]
    async fn settings_that_are_not_enough_are_refused_and_never_replaced_by_the_servers() {
        let (url, seen) = standin().await;
        let setup = setup(server_gateway(&url), endpoints(Some(url.clone()), false));
        let (focus, nfeio) = (CompanyId::generate(), CompanyId::generate());
        let user = contract::actor();
        setup
            .settings
            .replace(
                focus,
                &user,
                GatewaySettingsInput {
                    focus_token_homologation: secret("token-of-homologation-1"),
                    ..input(GatewayKind::Focus)
                },
            )
            .await
            .unwrap();
        setup
            .settings
            .replace(
                nfeio,
                &user,
                GatewaySettingsInput {
                    nfeio_api_key: secret("the-nfeio-api-key-0001"),
                    nfeio_company_homologation: Some("company-1".to_owned()),
                    ..input(GatewayKind::NfeIo)
                },
            )
            .await
            .unwrap();
        let missing = |error: ApplicationError| match error {
            ApplicationError::FiscalRule(text) => text,
            other => panic!("a fiscal rule was expected, got {other:?}"),
        };

        let production = issuer(focus, SERVER_CNPJ, FiscalEnvironment::Production);
        let text = missing(setup.resolver.resolve(&production).await.err().unwrap());
        assert!(text.contains("Focus NFe token") && text.contains("production"));
        let production = issuer(nfeio, SERVER_CNPJ, FiscalEnvironment::Production);
        let text = missing(setup.resolver.resolve(&production).await.err().unwrap());
        assert!(text.contains("NFE.io company id") && text.contains("production"));
        assert!(seen.lock().unwrap().is_empty());
    }

    /// A secret that no longer opens (the key changed) is a refusal that names the field, not a fall back to the server's.
    #[tokio::test]
    async fn a_secret_that_does_not_open_is_refused_and_never_replaced_by_the_servers() {
        let (url, seen) = standin().await;
        let setup = setup(server_gateway(&url), endpoints(Some(url.clone()), false));
        let company = CompanyId::generate();
        setup
            .settings
            .replace(
                company,
                &contract::actor(),
                GatewaySettingsInput {
                    focus_token_homologation: secret("token-of-homologation-1"),
                    ..input(GatewayKind::Focus)
                },
            )
            .await
            .unwrap();
        let other_key = "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100";
        let resolver = CompanyFiscalGateways::new(
            server_gateway(&url),
            GatewaySettingsService::new(
                setup.repository.clone(),
                RingSecretCipher::from_hex(other_key).unwrap(),
            ),
            endpoints(Some(url), false),
        );
        let issuer = issuer(company, SERVER_CNPJ, FiscalEnvironment::Homologation);
        assert!(matches!(
            resolver.resolve(&issuer).await.err(),
            Some(ApplicationError::SecretUnreadable(
                "focus_token_homologation"
            ))
        ));
        // And a server that has no key at all does not open it either.
        let without_key = CompanyFiscalGateways::new(
            server_gateway("http://127.0.0.1:1"),
            GatewaySettingsService::new(setup.repository.clone(), NoSecretCipher),
            endpoints(None, false),
        );
        assert!(matches!(
            without_key.resolve(&issuer).await.err(),
            Some(ApplicationError::SecretUnreadable(_))
        ));
        assert!(seen.lock().unwrap().is_empty());
    }

    /// A production issue through NFE.io needs the operator's confirmation as well as the company's own flag.
    #[tokio::test]
    async fn a_production_issue_through_nfeio_needs_the_operators_confirmation_too() {
        let company = CompanyId::generate();
        let production = issuer(company, OWN_CNPJ, FiscalEnvironment::Production);
        let request = nfe_request(&production, "ref-1");
        let refused = |error: GatewayError| matches!(error, GatewayError::Unsupported(text) if text.contains("NFEIO_SEARCH_CONFIRMED"));
        let settings_of = |confirmed| GatewaySettingsInput {
            nfeio_api_key: secret("the-nfeio-api-key-0001"),
            nfeio_company_production: Some("company-1".to_owned()),
            nfeio_search_confirmed: confirmed,
            ..input(GatewayKind::NfeIo)
        };
        let user = contract::actor();

        // The company's flag alone: refused before anything is sent.
        let (url, seen) = standin().await;
        let setup_without = setup(server_gateway(&url), endpoints(Some(url.clone()), false));
        setup_without
            .settings
            .replace(company, &user, settings_of(true))
            .await
            .unwrap();
        let gateway = setup_without.resolver.resolve(&production).await.unwrap();
        assert!(refused(gateway.submit_nfe(&request).await.unwrap_err()));

        // The operator's alone: refused as well.
        let setup_only_operator = setup(server_gateway(&url), endpoints(Some(url.clone()), true));
        setup_only_operator
            .settings
            .replace(company, &user, settings_of(false))
            .await
            .unwrap();
        let gateway = setup_only_operator
            .resolver
            .resolve(&production)
            .await
            .unwrap();
        assert!(refused(gateway.submit_nfe(&request).await.unwrap_err()));
        assert!(seen.lock().unwrap().is_empty());

        // Both: it goes on to ask the gateway (and finds nothing it can use in the stand-in's answer).
        let setup_both = setup(server_gateway(&url), endpoints(Some(url.clone()), true));
        setup_both
            .settings
            .replace(company, &user, settings_of(true))
            .await
            .unwrap();
        let gateway = setup_both.resolver.resolve(&production).await.unwrap();
        assert!(!refused(
            gateway
                .submit_nfe(&request)
                .await
                .err()
                .unwrap_or(GatewayError::Disabled)
        ));
        assert!(!seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn what_the_page_says_is_the_gateway_in_effect() {
        let setup = setup(server_gateway("http://127.0.0.1:1"), endpoints(None, false));
        let company = CompanyId::generate();
        assert_eq!(setup.resolver.server_key().as_deref(), Some("focus"));
        assert_eq!(
            setup.resolver.key_for(company).await.as_deref(),
            Some("focus")
        );
        setup
            .settings
            .replace(
                company,
                &contract::actor(),
                GatewaySettingsInput {
                    nfeio_api_key: secret("the-nfeio-api-key-0001"),
                    ..input(GatewayKind::NfeIo)
                },
            )
            .await
            .unwrap();
        assert_eq!(
            setup.resolver.key_for(company).await.as_deref(),
            Some("nfeio")
        );
        assert_eq!(setup.resolver.server_key().as_deref(), Some("focus"));
    }
}
