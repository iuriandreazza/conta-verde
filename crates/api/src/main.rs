use std::time::Duration;

use conta_verde_api::{ApiConfig, Ports, Services, router};
use conta_verde_application::{
    AuthService, BalanceService, BankAccountService, CategoryService, CostCenterService,
    FinancialEntryService, MemberService, PartyService, PaymentMethodService, PaymentTermService,
    PostalCodeService, ProductService, RecurrenceService, TransferService,
};
use conta_verde_application::{EmailError, EmailMessage, EmailSender};
use conta_verde_infrastructure::{
    Argon2SecretService, BrasilApiCnpjRegistry, BrasilApiPostalCodeDirectory, CachedCnpjRegistry,
    LogEmailSender, PostgresBankAccountRepository, PostgresCategoryRepository,
    PostgresCostCenterRepository, PostgresFinancialEntryRepository, PostgresIdentityRepository,
    PostgresInvitationRepository, PostgresOpeningRepository, PostgresPartyRepository,
    PostgresPaymentMethodRepository, PostgresPaymentTermRepository, PostgresProductRepository,
    PostgresRecurrenceRepository, PostgresTransferRepository, RegisterReferences, SmtpEmailSender,
    connect_and_migrate,
};
use tracing_subscriber::EnvFilter;

/// The adapters of a running installation.
struct PostgresPorts;

impl Ports for PostgresPorts {
    type Parties = PostgresPartyRepository;
    type Registry = Option<CachedCnpjRegistry<BrasilApiCnpjRegistry>>;
    type Identity = PostgresIdentityRepository;
    type Secrets = Argon2SecretService;
    type Products = PostgresProductRepository;
    type CostCenters = PostgresCostCenterRepository;
    type Categories = PostgresCategoryRepository;
    type BankAccounts = PostgresBankAccountRepository;
    type PaymentMethods = PostgresPaymentMethodRepository;
    type PaymentTerms = PostgresPaymentTermRepository;
    type Entries = PostgresFinancialEntryRepository;
    type EntryReferences = RegisterReferences<
        PostgresCategoryRepository,
        PostgresCostCenterRepository,
        PostgresPartyRepository,
        PostgresBankAccountRepository,
        PostgresPaymentMethodRepository,
    >;
    type Transfers = PostgresTransferRepository;
    type Openings = PostgresOpeningRepository;
    type Recurrences = PostgresRecurrenceRepository;
    type PostalCodes = Option<BrasilApiPostalCodeDirectory>;
    type Invitations = PostgresInvitationRepository;
    type Emails = ConfiguredEmailSender;
}

const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const INVITATION_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const RECURRENCE_JOB_INTERVAL: Duration = Duration::from_secs(60 * 60);
const CNPJ_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let database_url = std::env::var("DATABASE_URL")?;
    // Loopback by default: put a TLS-terminating reverse proxy in front before exposing the API.
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    let config = ApiConfig {
        cookie_secure: parse_flag("COOKIE_SECURE")?,
    };

    let pool = connect_and_migrate(&database_url).await?;
    let parties = PartyService::new(
        PostgresPartyRepository::new(pool.clone()),
        cnpj_registry_from_env()?,
    );
    let products = ProductService::new(PostgresProductRepository::new(pool.clone()));
    let cost_centers = CostCenterService::new(PostgresCostCenterRepository::new(pool.clone()));
    let categories = CategoryService::new(PostgresCategoryRepository::new(pool.clone()));
    let bank_accounts = BankAccountService::new(PostgresBankAccountRepository::new(pool.clone()));
    let payment_methods =
        PaymentMethodService::new(PostgresPaymentMethodRepository::new(pool.clone()));
    let payment_terms = PaymentTermService::new(PostgresPaymentTermRepository::new(pool.clone()));
    let references = || {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresPaymentMethodRepository::new(pool.clone()),
        )
    };
    let entries = FinancialEntryService::new(
        PostgresFinancialEntryRepository::new(pool.clone()),
        references(),
    );
    let transfers =
        TransferService::new(PostgresTransferRepository::new(pool.clone()), references());
    let recurrence_service = || {
        RecurrenceService::new(
            PostgresRecurrenceRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(),
            ),
            references(),
        )
    };
    let recurrences = recurrence_service();
    tokio::spawn(generate_recurring_entries(recurrence_service()));
    let balances = BalanceService::new(
        PostgresBankAccountRepository::new(pool.clone()),
        PostgresFinancialEntryRepository::new(pool.clone()),
        PostgresTransferRepository::new(pool.clone()),
        PostgresOpeningRepository::new(pool.clone()),
    );
    let postal_codes = PostalCodeService::new(postal_code_directory_from_env()?);
    // Invitation links lead to the web app, which is where the person accepts.
    let public_url = std::env::var("PUBLIC_URL").unwrap_or_else(|_| {
        tracing::warn!(
            "PUBLIC_URL is not set: invitation links will lead to http://localhost:5173"
        );
        "http://localhost:5173".to_owned()
    });
    let members = MemberService::new(
        PostgresInvitationRepository::new(pool.clone()),
        PostgresIdentityRepository::new(pool.clone()),
        email_sender_from_env()?,
        Argon2SecretService::new()?,
        INVITATION_LIFETIME,
        &public_url,
    );
    let auth = AuthService::new(
        PostgresIdentityRepository::new(pool),
        Argon2SecretService::new()?,
        SESSION_LIFETIME,
    );
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(%bind_addr, "listening");
    axum::serve(
        listener,
        router::<PostgresPorts>(
            Services {
                parties,
                products,
                cost_centers,
                categories,
                bank_accounts,
                payment_methods,
                payment_terms,
                entries,
                transfers,
                recurrences,
                balances,
                postal_codes,
                members,
                auth,
            },
            config,
        ),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

/// The scheduled job of ADR 0033: once an hour, generates the recurring entries that fall due in the
/// next two months. It is safe to repeat and to run in several instances, so a failed pass only waits for
/// the next one.
async fn generate_recurring_entries<R, E, F>(service: RecurrenceService<R, E, F>)
where
    R: conta_verde_application::RecurrenceRepository,
    E: conta_verde_application::FinancialEntryRepository,
    F: conta_verde_application::EntryReferences,
{
    let mut tick = tokio::time::interval(RECURRENCE_JOB_INTERVAL);
    loop {
        tick.tick().await;
        match service.generate_all_due(conta_verde_api::today()).await {
            Ok(report) if report.made > 0 || report.failed > 0 => {
                tracing::info!(
                    made = report.made,
                    failed = report.failed,
                    "recurring entries generated"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::error!(%error, "generating recurring entries failed"),
        }
    }
}

/// The mail sender the operator chose: an SMTP server when `SMTP_URL` is set, and otherwise the log.
enum ConfiguredEmailSender {
    // Boxed: the transport is large next to the unit of the log adapter.
    Smtp(Box<SmtpEmailSender>),
    Log(LogEmailSender),
}

impl EmailSender for ConfiguredEmailSender {
    async fn send(&self, message: &EmailMessage) -> Result<(), EmailError> {
        match self {
            Self::Smtp(sender) => sender.send(message).await,
            Self::Log(sender) => sender.send(message).await,
        }
    }
}

/// `SMTP_URL` and `SMTP_FROM` together send real e-mail; with neither, messages go to the log and
/// nothing is sent (the web app then shows an invitation's link to pass on). One without the other
/// is a mistake, and stops the server instead of silently not sending.
fn email_sender_from_env() -> Result<ConfiguredEmailSender, Box<dyn std::error::Error + Send + Sync>>
{
    // `SMTP_URL=` in an env file or a compose file means "none", not "an empty server".
    let setting = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    };
    email_sender(
        setting("SMTP_URL").as_deref(),
        setting("SMTP_FROM").as_deref(),
    )
}

fn email_sender(
    url: Option<&str>,
    from: Option<&str>,
) -> Result<ConfiguredEmailSender, Box<dyn std::error::Error + Send + Sync>> {
    match (url, from) {
        (Some(url), Some(from)) if !url.is_empty() => {
            tracing::info!("e-mail is sent through the SMTP server of SMTP_URL");
            Ok(ConfiguredEmailSender::Smtp(Box::new(SmtpEmailSender::new(
                url, from,
            )?)))
        }
        (None, None) => {
            tracing::warn!("SMTP_URL is not set: e-mail is only logged, nothing is sent");
            Ok(ConfiguredEmailSender::Log(LogEmailSender))
        }
        _ => Err("SMTP_URL and SMTP_FROM must be set together".into()),
    }
}

/// `None` when the lookup is switched off. A company the registry already answered for is
/// remembered for an hour, which keeps retries and repeated suppliers off the shared rate limit.
fn cnpj_registry_from_env() -> Result<
    Option<CachedCnpjRegistry<BrasilApiCnpjRegistry>>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let setting = std::env::var("CNPJ_LOOKUP").ok();
    if !lookup_enabled("CNPJ_LOOKUP", setting.as_deref())? {
        tracing::warn!("CNPJ lookup is off: CNPJs are validated by check digits only");
        return Ok(None);
    }
    tracing::info!("CNPJs are checked against BrasilAPI (Receita Federal open data)");
    Ok(Some(CachedCnpjRegistry::new(
        BrasilApiCnpjRegistry::new()?,
        CNPJ_CACHE_TTL,
    )))
}

/// `None` when the lookup is switched off. Only a postal code is sent to the third party.
fn postal_code_directory_from_env()
-> Result<Option<BrasilApiPostalCodeDirectory>, Box<dyn std::error::Error + Send + Sync>> {
    let setting = std::env::var("CEP_LOOKUP").ok();
    if !lookup_enabled("CEP_LOOKUP", setting.as_deref())? {
        tracing::warn!("CEP lookup is off: addresses are filled in by hand");
        return Ok(None);
    }
    tracing::info!("postal codes are looked up on BrasilAPI (Correios and other public sources)");
    Ok(Some(BrasilApiPostalCodeDirectory::new()?))
}

// `off` is for installations that must not send a CNPJ or a postal code to a third party, or run
// without internet. Strict like `parse_flag`, so a typo is not read as "off".
fn lookup_enabled(name: &str, setting: Option<&str>) -> Result<bool, String> {
    match setting {
        None | Some("brasilapi") => Ok(true),
        Some("off") => Ok(false),
        Some(other) => Err(format!(
            "{name} must be `brasilapi` or `off`, got `{other}`"
        )),
    }
}

// Strict on purpose: a typo such as `TRUE` must not silently turn off a security setting.
fn parse_flag(name: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    match std::env::var(name).as_deref() {
        Err(_) | Ok("false") => Ok(false),
        Ok("true") => Ok(true),
        Ok(other) => Err(format!("{name} must be `true` or `false`, got `{other}`").into()),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lookup_is_on_by_default_and_off_only_when_asked() {
        assert_eq!(lookup_enabled("CNPJ_LOOKUP", None), Ok(true));
        assert_eq!(lookup_enabled("CNPJ_LOOKUP", Some("brasilapi")), Ok(true));
        assert_eq!(lookup_enabled("CNPJ_LOOKUP", Some("off")), Ok(false));
    }

    #[test]
    fn e_mail_goes_to_the_log_without_a_server_and_through_smtp_with_one() {
        const FROM: &str = "conta-verde <no-reply@erp.example>";
        assert!(matches!(
            email_sender(None, None),
            Ok(ConfiguredEmailSender::Log(_))
        ));
        assert!(matches!(
            email_sender(Some("smtp://localhost:1025"), Some(FROM)),
            Ok(ConfiguredEmailSender::Smtp(_))
        ));
    }

    #[test]
    fn half_a_mail_setting_stops_the_server_instead_of_silently_not_sending() {
        const FROM: &str = "conta-verde <no-reply@erp.example>";
        for (url, from) in [
            (Some("smtp://localhost:1025"), None),
            (None, Some(FROM)),
            (Some("not a url"), Some(FROM)),
            (Some("smtp://localhost:1025"), Some("not an address")),
        ] {
            assert!(email_sender(url, from).is_err(), "{url:?} {from:?}");
        }
    }

    #[test]
    fn a_typo_is_an_error_not_a_silent_off() {
        for typo in ["OFF", "false", "", "brasil-api"] {
            let error = lookup_enabled("CEP_LOOKUP", Some(typo)).unwrap_err();
            // The message names the setting that was wrong.
            assert!(error.starts_with("CEP_LOOKUP must be"), "{typo}");
        }
    }
}
