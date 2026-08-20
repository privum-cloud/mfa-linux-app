//! The encrypted account store.

mod crypto;
mod document;
mod file;

pub use crypto::{derive_key, open, seal, KdfParams, VaultError};
pub use document::{VaultDocument, TOMBSTONE_RETENTION_DAYS};
pub use file::{load_document, save_document};
