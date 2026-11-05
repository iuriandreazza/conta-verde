use std::time::Duration;

use conta_verde_api::{ApiConfig, Ports, Services, router};
use conta_verde_application::{
    AuthService, BankAccountService, CategoryService, CostCenterService, PartyService,
    PaymentMethodService, PaymentTermService, PostalCodeService, ProductService,
};
use conta_verde_infrastructure::{
    Argon2SecretService, BrasilApiCnpjRegistry, BrasilApiPostalCodeDirectory, CachedCnpjRegistry,
    PostgresBankAccountRepository, PostgresCategoryRepository, PostgresCostCenterRepository,
    PostgresIdentityRepository, PostgresPartyRepository, PostgresPaymentMethodRepository,
    PostgresPaymentTermRepository, PostgresProductRepository, connect_and_migrate,
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
    type PostalCodes = Option<BrasilApiPostalCodeDirectory>;
}

const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
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
    let postal_codes = PostalCodeService::new(postal_code_directory_from_env()?);
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
                postal_codes,
                auth,
            },
            config,
        ),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
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
    fn a_typo_is_an_error_not_a_silent_off() {
        for typo in ["OFF", "false", "", "brasil-api"] {
            let error = lookup_enabled("CEP_LOOKUP", Some(typo)).unwrap_err();
            // The message names the setting that was wrong.
            assert!(error.starts_with("CEP_LOOKUP must be"), "{typo}");
        }
    }
}
