use std::time::Duration;

use conta_verde_api::{ApiConfig, router};
use conta_verde_application::{AuthService, PartyService};
use conta_verde_infrastructure::{
    Argon2SecretService, BrasilApiCnpjRegistry, PostgresIdentityRepository,
    PostgresPartyRepository, connect_and_migrate,
};
use tracing_subscriber::EnvFilter;

const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);

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
    let auth = AuthService::new(
        PostgresIdentityRepository::new(pool),
        Argon2SecretService::new()?,
        SESSION_LIFETIME,
    );
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(%bind_addr, "listening");
    axum::serve(listener, router(parties, auth, config))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

// `off` is for installations that must not send CNPJs to a third party or run without internet:
// every CNPJ is then stored as unverified. Strict like `parse_flag`, so a typo is not read as "off".
fn cnpj_registry_from_env()
-> Result<Option<BrasilApiCnpjRegistry>, Box<dyn std::error::Error + Send + Sync>> {
    match std::env::var("CNPJ_LOOKUP").as_deref() {
        Err(_) | Ok("brasilapi") => {
            tracing::info!("CNPJs are checked against BrasilAPI (Receita Federal open data)");
            Ok(Some(BrasilApiCnpjRegistry::new()?))
        }
        Ok("off") => {
            tracing::warn!("CNPJ lookup is off: CNPJs are validated by check digits only");
            Ok(None)
        }
        Ok(other) => Err(format!("CNPJ_LOOKUP must be `brasilapi` or `off`, got `{other}`").into()),
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
