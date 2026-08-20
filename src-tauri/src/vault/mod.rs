//! The encrypted account store.

mod crypto;

pub use crypto::{derive_key, open, seal, KdfParams, VaultError};
