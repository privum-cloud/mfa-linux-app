//! The encrypted account store.

mod crypto;
mod file;

pub use crypto::{derive_key, open, seal, KdfParams, VaultError};
pub use file::{load_document, save_document};
