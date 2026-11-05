mod brasil_api;
#[cfg(test)]
mod contract;
mod memory;
mod postgres;
mod postgres_identity;
mod secrets;

pub use brasil_api::BrasilApiCnpjRegistry;
pub use memory::{InMemoryCnpjRegistry, InMemoryIdentityRepository, InMemoryPartyRepository};
pub use postgres::{PostgresPartyRepository, connect_and_migrate};
pub use postgres_identity::PostgresIdentityRepository;
pub use secrets::Argon2SecretService;
