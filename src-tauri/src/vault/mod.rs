//! The encrypted account store.

mod crypto;
mod document;
mod file;
mod location;
mod manager;
mod settings;

pub use crypto::{derive_key, open, seal, KdfParams, VaultError};
pub use document::{VaultDocument, TOMBSTONE_RETENTION_DAYS};
pub use file::{load_document, save_document, save_document_to};
pub use location::{config_path, Location};
pub use manager::{default_vault_path, VaultManager};
pub use settings::Settings;
