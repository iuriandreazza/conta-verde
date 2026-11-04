use std::sync::Arc;

use conta_verde_api::router;
use conta_verde_application::PartyService;
use conta_verde_infrastructure::{PostgresPartyRepository, migrate};
use sqlx::PgPool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_owned());

    let pool = PgPool::connect(&database_url).await?;
    migrate(&pool).await?;

    let service = Arc::new(PartyService::new(PostgresPartyRepository::new(pool)));
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    println!("listening on {bind_addr}");
    axum::serve(listener, router(service)).await?;
    Ok(())
}
