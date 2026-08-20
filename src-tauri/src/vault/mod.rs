//! The encrypted account store.

mod crypto;
mod document;
mod file;
mod manager;
mod settings;

pub use crypto::{derive_key, open, seal, KdfParams, VaultError};
pub use document::{VaultDocument, TOMBSTONE_RETENTION_DAYS};
pub use file::{load_document, save_document};
pub use manager::{default_vault_path, VaultManager};
pub use settings::Settings;
