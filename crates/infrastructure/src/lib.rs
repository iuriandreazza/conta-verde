mod brasil_api;
mod cached_registry;
#[cfg(test)]
mod contract;
mod memory;
mod memory_audit;
mod memory_list;
mod memory_products;
mod postgres;
mod postgres_audit;
mod postgres_identity;
mod postgres_products;
mod secrets;

pub use brasil_api::BrasilApiCnpjRegistry;
pub use cached_registry::CachedCnpjRegistry;
pub use memory::{InMemoryCnpjRegistry, InMemoryIdentityRepository, InMemoryPartyRepository};
pub use memory_products::InMemoryProductRepository;
pub use postgres::{PostgresPartyRepository, connect_and_migrate};
pub use postgres_identity::PostgresIdentityRepository;
pub use postgres_products::PostgresProductRepository;
pub use secrets::Argon2SecretService;
