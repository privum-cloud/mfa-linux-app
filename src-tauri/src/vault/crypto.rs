//! Key derivation and authenticated encryption for the vault.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

/// Argon2id cost parameters.
///
/// These are read back out of the vault header so that a file written with one
/// profile still opens after the defaults change. That also means they arrive
/// from a file we do not control, which is why `validated` exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KdfParams {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        // The OWASP profile for Argon2id: 19 MiB, two iterations, one lane.
        Self {
            m_cost: 19_456,
            t_cost: 2,
            p_cost: 1,
        }
    }
}

/// The lower bound is Argon2's own minimum; the upper bound is what keeps a
/// hostile or corrupted header from asking for gigabytes.
const MIN_M_COST: u32 = 8;
const MAX_M_COST: u32 = 1_048_576; // 1 GiB
const MAX_T_COST: u32 = 16;
const MAX_P_COST: u32 = 16;

#[derive(Debug, serde::Serialize)]
pub enum VaultError {
    Crypto,
    BadFormat,
    Locked,
    KdfParams,
    Io(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultError::Crypto => write!(f, "that password does not open this vault"),
            VaultError::BadFormat => write!(f, "this file is not a Tessera vault"),
            VaultError::Locked => write!(f, "the vault is locked"),
            VaultError::KdfParams => write!(f, "the vault header declares unusable key settings"),
            VaultError::Io(e) => write!(f, "could not read or write the vault: {e}"),
        }
    }
}

impl std::error::Error for VaultError {}

impl KdfParams {
    /// Bound every field before Argon2 sees it.
    ///
    /// These values are read from the vault header, so they are attacker-
    /// controlled in the case that matters: a file handed to the user. An
    /// unbounded `m_cost` is a memory-exhaustion request, and a zero in any
    /// field makes `Params::new` fail — which behind an `.expect()` would be a
    /// panic rather than an error message.
    pub fn validated(self) -> Result<Self, VaultError> {
        let sane = (MIN_M_COST..=MAX_M_COST).contains(&self.m_cost)
            && (1..=MAX_T_COST).contains(&self.t_cost)
            && (1..=MAX_P_COST).contains(&self.p_cost);

        if sane {
            Ok(self)
        } else {
            Err(VaultError::KdfParams)
        }
    }
}

/// Stretch the master password into a 256-bit key.
pub fn derive_key(
    password: &str,
    salt: &[u8],
    params: KdfParams,
) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    let params = params.validated()?;
    let argon_params = Params::new(params.m_cost, params.t_cost, params.p_cost, Some(32))
        .map_err(|_| VaultError::KdfParams)?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut_slice())
        .map_err(|_| VaultError::KdfParams)?;
    Ok(key)
}

/// Encrypt and authenticate. The nonce must never repeat for a given key, which
/// is why `file::save_document` draws a fresh one on every write.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .encrypt(Nonce::from_slice(nonce), plaintext)
        .expect("AES-256-GCM encryption does not fail for a valid key and nonce")
}

/// Decrypt and verify. A failure here means the password was wrong or the file
/// was altered; the two are indistinguishable by design.
pub fn open(key: &[u8; 32], nonce: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>, VaultError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| VaultError::Crypto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_then_open_round_trips() {
        let key = derive_key("correct horse", &[7u8; 16], KdfParams::default()).unwrap();
        let sealed = seal(&key, &[3u8; 12], b"the account list");
        assert_eq!(
            open(&key, &[3u8; 12], &sealed).unwrap(),
            b"the account list"
        );
    }

    #[test]
    fn the_wrong_password_does_not_open_the_vault() {
        let params = KdfParams::default();
        let right = derive_key("correct horse", &[7u8; 16], params).unwrap();
        let wrong = derive_key("correct hoarse", &[7u8; 16], params).unwrap();
        let sealed = seal(&right, &[3u8; 12], b"the account list");
        assert!(matches!(
            open(&wrong, &[3u8; 12], &sealed),
            Err(VaultError::Crypto)
        ));
    }

    #[test]
    fn a_tampered_ciphertext_is_rejected() {
        // The GCM tag is what makes a wrong password distinguishable from a
        // corrupted file, so this must fail rather than return garbage.
        let key = derive_key("x", &[1u8; 16], KdfParams::default()).unwrap();
        let mut sealed = seal(&key, &[0u8; 12], b"abc");
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        assert!(open(&key, &[0u8; 12], &sealed).is_err());
    }

    #[test]
    fn hostile_kdf_params_are_refused_rather_than_panicking() {
        // These arrive from the vault header, which we do not control. Remota
        // passes them straight to Argon2 behind an .expect(); a file asking for
        // four gigabytes of memory, or for zero lanes, must be an error here.
        for bad in [
            KdfParams {
                m_cost: 0,
                t_cost: 2,
                p_cost: 1,
            },
            KdfParams {
                m_cost: 4_000_000,
                t_cost: 2,
                p_cost: 1,
            },
            KdfParams {
                m_cost: 19_456,
                t_cost: 0,
                p_cost: 1,
            },
            KdfParams {
                m_cost: 19_456,
                t_cost: 2,
                p_cost: 0,
            },
            KdfParams {
                m_cost: 19_456,
                t_cost: 999,
                p_cost: 1,
            },
        ] {
            assert!(
                matches!(bad.validated(), Err(VaultError::KdfParams)),
                "{bad:?} should have been refused"
            );
            assert!(
                derive_key("pw", &[0u8; 16], bad).is_err(),
                "{bad:?} reached Argon2"
            );
        }
    }

    #[test]
    fn the_default_params_are_accepted() {
        assert_eq!(
            KdfParams::default().validated().unwrap(),
            KdfParams::default()
        );
    }
}
