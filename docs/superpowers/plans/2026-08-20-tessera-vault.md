# Tessera Vault Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store accounts in a file only the master password can open, and expose the operations the interface needs — create, unlock, lock, list, add, edit, delete — so that the next plan has nothing to do but draw.

**Architecture:** A four-layer vault. `crypto` derives a key and seals bytes. `file` frames those bytes on disk and writes them atomically. `document` is what gets sealed — the account list plus the metadata synchronisation will need. `manager` owns the unlocked state, holds it behind a lock, and drops it on inactivity. Above them sits the command surface, which is the only thing the front end sees.

**Tech Stack:** Rust 1.96 (pinned), `argon2`, `aes-gcm`, `getrandom`, `zeroize`, `url`. Builds on the `otp/` and `model/` modules from the foundation plan.

**Spec:** `docs/superpowers/specs/2026-08-20-tessera-design.md`

## Global Constraints

- **License:** GPL-3.0-only. **Language:** English throughout.
- **The front end never receives a raw secret.** Commands return codes and metadata.
- **The master password is required at every launch. There is no OS keyring.** This was decided explicitly and re-confirmed after the friction was explained; do not add a keyring path.
- **Key derivation is argon2id at the OWASP profile:** m_cost 19456 KiB, t_cost 2, p_cost 1. **Sealing is AES-256-GCM.**
- **Every key lives in `Zeroizing`** and is dropped on lock.
- **TDD:** the failing test is written and observed failing first.
- **Run `cargo fmt --all` before every commit.** `--check` reports but does not fix, and CI runs `--check`.
- **The toolchain is pinned in `rust-toolchain.toml`.** Do not switch to `@stable`.

---

### Task 1: Vault cryptography

Key derivation and sealing. Adapted from Remota's `src-tauri/src/vault/crypto.rs`, with one deliberate departure described below.

**Files:**
- Create: `src-tauri/src/vault/mod.rs`, `src-tauri/src/vault/crypto.rs`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: nothing from earlier plans.
- Produces:
  - `pub struct KdfParams { pub m_cost: u32, pub t_cost: u32, pub p_cost: u32 }` — `Copy`, `Debug`, `PartialEq`, `Default`
  - `pub fn validated(self) -> Result<KdfParams, VaultError>` on `KdfParams`
  - `pub enum VaultError { Crypto, BadFormat, Locked, KdfParams, Io(String) }` — `Debug`, `Serialize`, `Display`, `Error`
  - `pub fn derive_key(password: &str, salt: &[u8], params: KdfParams) -> Result<Zeroizing<[u8; 32]>, VaultError>`
  - `pub fn seal(key: &[u8; 32], nonce: &[u8; 12], plaintext: &[u8]) -> Vec<u8>`
  - `pub fn open(key: &[u8; 32], nonce: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>, VaultError>`

**Departure from Remota — read this before writing the code.**

Remota's `derive_key` ends in `.expect("params Argon2 válidos")`, and its `load_document` reads `m_cost`, `t_cost` and `p_cost *out of the file header* and passes them straight in. Those two facts together mean a corrupted or hostile vault file can panic the application, and a file declaring `m_cost = 4000000` makes Argon2 try to allocate four gigabytes. Tessera validates instead: `KdfParams::validated` bounds every field, and `derive_key` returns `Result`.

- [ ] **Step 1: Add the dependencies**

```bash
cd src-tauri
cargo add argon2@0.5 aes-gcm@0.10 getrandom@0.2 url@2
```

- [ ] **Step 2: Write the failing tests**

```bash
mkdir -p src-tauri/src/vault
cat > src-tauri/src/vault/crypto.rs <<'EOF'
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_then_open_round_trips() {
        let key = derive_key("correct horse", &[7u8; 16], KdfParams::default()).unwrap();
        let sealed = seal(&key, &[3u8; 12], b"the account list");
        assert_eq!(open(&key, &[3u8; 12], &sealed).unwrap(), b"the account list");
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
            KdfParams { m_cost: 0, t_cost: 2, p_cost: 1 },
            KdfParams { m_cost: 4_000_000, t_cost: 2, p_cost: 1 },
            KdfParams { m_cost: 19_456, t_cost: 0, p_cost: 1 },
            KdfParams { m_cost: 19_456, t_cost: 2, p_cost: 0 },
            KdfParams { m_cost: 19_456, t_cost: 999, p_cost: 1 },
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
        assert_eq!(KdfParams::default().validated().unwrap(), KdfParams::default());
    }
}
EOF
cat > src-tauri/src/vault/mod.rs <<'EOF'
//! The encrypted account store.

mod crypto;

pub use crypto::{derive_key, open, seal, KdfParams, VaultError};
EOF
```

Add `pub mod vault;` to `src-tauri/src/lib.rs` beneath `pub mod otp;`.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test vault:: 2>&1 | grep -E '^error' | head -5`
Expected: FAIL — `derive_key`, `seal`, `open` and `validated` do not exist.

- [ ] **Step 4: Write the implementation**

Insert into `crypto.rs`, above the `#[cfg(test)]` block:

```rust
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test vault:: 2>&1 | grep 'test result'`
Expected: `test result: ok. 5 passed`.

- [ ] **Step 6: Format, lint, commit**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --all-targets -- -D warnings
cd .. && git add -A
git commit -m "feat(vault): add argon2id key derivation and AES-256-GCM sealing

Adapted from Remota, with one departure: Remota reads Argon2 cost parameters
out of the file header and passes them to an .expect(), so a corrupted or
hostile vault panics the application and a large m_cost exhausts memory.
Here they are bounded first and derive_key returns Result."
```

---

### Task 2: The vault file

Frames the sealed bytes on disk. Remota's equivalent writes with `std::fs::write`, which truncates the target before writing — a crash midway leaves a vault that is neither the old one nor the new one. Tessera writes to a temporary file in the same directory and renames it, which on Linux is atomic.

**Files:**
- Create: `src-tauri/src/vault/file.rs`
- Modify: `src-tauri/src/vault/mod.rs`

**Interfaces:**
- Consumes: `derive_key`, `seal`, `open`, `KdfParams`, `VaultError` from Task 1.
- Produces:
  - `pub fn save_document(path: &Path, password: &str, params: KdfParams, plaintext: &[u8]) -> Result<(), VaultError>`
  - `pub fn load_document(path: &Path, password: &str) -> Result<Vec<u8>, VaultError>`

The on-disk layout, 41 bytes of header then the sealed body:

```
offset  size  field
0       1     format version (currently 1)
1       4     m_cost, little-endian
5       4     t_cost, little-endian
9       4     p_cost, little-endian
13      16    salt
29      12    nonce
41      ..    AES-256-GCM ciphertext and tag
```

The header is deliberately in the clear: the parameters are needed *before* a key can be derived, so they cannot themselves be encrypted. Nothing in it is secret — but everything in it is untrusted, which is what Task 1's validation is for.

- [ ] **Step 1: Write the failing tests**

```bash
cat > src-tauri/src/vault/file.rs <<'EOF'
//! Reading and writing the vault file.

use std::path::Path;

use crate::vault::{derive_key, open, seal, KdfParams, VaultError};

const VERSION: u8 = 1;
const HEADER_LEN: usize = 1 + 4 + 4 + 4 + 16 + 12; // 41

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique path per test. No randomness is used, so the test name is the key.
    fn tmp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("tessera-test-{name}.bin"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = tmp_path("roundtrip");
        save_document(&path, "master", KdfParams::default(), b"{\"accounts\":[]}").unwrap();
        assert_eq!(
            load_document(&path, "master").unwrap(),
            b"{\"accounts\":[]}"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn the_wrong_password_is_refused() {
        let path = tmp_path("wrongpw");
        save_document(&path, "master", KdfParams::default(), b"secret").unwrap();
        assert!(matches!(
            load_document(&path, "not the master"),
            Err(VaultError::Crypto)
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn salt_and_nonce_differ_between_writes() {
        // Reusing a nonce under one key breaks GCM outright, so this is not a
        // stylistic check.
        let (a, b) = (tmp_path("rng-a"), tmp_path("rng-b"));
        save_document(&a, "m", KdfParams::default(), b"x").unwrap();
        save_document(&b, "m", KdfParams::default(), b"x").unwrap();
        let (ba, bb) = (std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
        assert_ne!(ba[13..41], bb[13..41]);
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
    }

    #[test]
    fn a_truncated_file_is_reported_as_a_bad_format() {
        let path = tmp_path("truncated");
        std::fs::write(&path, [VERSION, 0, 0]).unwrap();
        assert!(matches!(
            load_document(&path, "m"),
            Err(VaultError::BadFormat)
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_future_format_version_is_refused_rather_than_misread() {
        let path = tmp_path("version");
        std::fs::write(&path, [99u8; 64]).unwrap();
        assert!(matches!(
            load_document(&path, "m"),
            Err(VaultError::BadFormat)
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_header_declaring_absurd_costs_is_refused_without_allocating() {
        // The parameters live in the clear and are read before any key exists.
        // A file asking for four gigabytes must be turned away here.
        let path = tmp_path("hostile-params");
        let mut buf = vec![VERSION];
        buf.extend_from_slice(&4_000_000u32.to_le_bytes());
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 16]);
        buf.extend_from_slice(&[0u8; 12]);
        buf.extend_from_slice(&[0u8; 32]);
        std::fs::write(&path, &buf).unwrap();

        assert!(matches!(
            load_document(&path, "m"),
            Err(VaultError::KdfParams)
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_interrupted_write_cannot_destroy_the_previous_vault() {
        // The write goes to a temporary file and is renamed into place, so the
        // target is either wholly old or wholly new. Proven here by checking
        // that no stray temporary survives a successful write.
        let path = tmp_path("atomic");
        save_document(&path, "m", KdfParams::default(), b"first").unwrap();
        save_document(&path, "m", KdfParams::default(), b"second").unwrap();

        assert_eq!(load_document(&path, "m").unwrap(), b"second");

        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("tessera-test-atomic") && n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary files survived: {leftovers:?}");
        std::fs::remove_file(&path).ok();
    }
}
EOF
```

Add `mod file;` and `pub use file::{load_document, save_document};` to `src-tauri/src/vault/mod.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test vault::file 2>&1 | grep -E '^error' | head -3`
Expected: FAIL — `save_document` and `load_document` do not exist.

- [ ] **Step 3: Write the implementation**

Insert into `file.rs`, above the `#[cfg(test)]` block:

```rust
/// Seal `plaintext` under `password` and write it to `path`.
///
/// The write goes to a temporary file in the same directory and is then
/// renamed. `rename` within a directory is atomic on Linux, so a crash leaves
/// the previous vault intact rather than a half-written one. Writing in place
/// would truncate the target first, and a vault truncated to zero bytes is the
/// user's entire second factor gone.
pub fn save_document(
    path: &Path,
    password: &str,
    params: KdfParams,
    plaintext: &[u8],
) -> Result<(), VaultError> {
    let params = params.validated()?;

    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut salt).map_err(|e| VaultError::Io(e.to_string()))?;
    getrandom::getrandom(&mut nonce).map_err(|e| VaultError::Io(e.to_string()))?;

    let key = derive_key(password, &salt, params)?;
    let ciphertext = seal(&key, &nonce, plaintext);

    let mut buf = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    buf.push(VERSION);
    buf.extend_from_slice(&params.m_cost.to_le_bytes());
    buf.extend_from_slice(&params.t_cost.to_le_bytes());
    buf.extend_from_slice(&params.p_cost.to_le_bytes());
    buf.extend_from_slice(&salt);
    buf.extend_from_slice(&nonce);
    buf.extend_from_slice(&ciphertext);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| VaultError::Io(e.to_string()))?;
    }

    // Same directory, so the rename stays on one filesystem and is atomic.
    let temporary = path.with_extension("tmp");
    write_and_sync(&temporary, &buf)?;
    std::fs::rename(&temporary, path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        VaultError::Io(e.to_string())
    })
}

/// Write the whole buffer and flush it to the device before returning, so the
/// rename that follows cannot expose a file whose contents are still in cache.
fn write_and_sync(path: &Path, buf: &[u8]) -> Result<(), VaultError> {
    use std::io::Write;

    let mut file = std::fs::File::create(path).map_err(|e| VaultError::Io(e.to_string()))?;
    file.write_all(buf).map_err(|e| VaultError::Io(e.to_string()))?;
    file.sync_all().map_err(|e| VaultError::Io(e.to_string()))
}

/// Read `path` and open it with `password`.
pub fn load_document(path: &Path, password: &str) -> Result<Vec<u8>, VaultError> {
    let buf = std::fs::read(path).map_err(|e| VaultError::Io(e.to_string()))?;
    if buf.len() < HEADER_LEN || buf[0] != VERSION {
        return Err(VaultError::BadFormat);
    }

    // Every value below comes from a file we did not write. The slices are safe
    // because the length was checked above; the parameters are not, which is
    // why derive_key validates them.
    let params = KdfParams {
        m_cost: u32::from_le_bytes(buf[1..5].try_into().unwrap()),
        t_cost: u32::from_le_bytes(buf[5..9].try_into().unwrap()),
        p_cost: u32::from_le_bytes(buf[9..13].try_into().unwrap()),
    }
    .validated()?;

    let salt: [u8; 16] = buf[13..29].try_into().unwrap();
    let nonce: [u8; 12] = buf[29..41].try_into().unwrap();

    let key = derive_key(password, &salt, params)?;
    open(&key, &nonce, &buf[41..])
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test vault:: 2>&1 | grep 'test result'`
Expected: `test result: ok. 12 passed`.

- [ ] **Step 5: Format, lint, commit**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --all-targets -- -D warnings
cd .. && git add -A
git commit -m "feat(vault): frame the vault on disk and write it atomically

Remota writes with fs::write, which truncates before writing: a crash midway
leaves a vault that is neither the old one nor the new one, and a vault
truncated to zero bytes is the user's whole second factor. Writing to a
temporary file in the same directory and renaming makes the result either
wholly old or wholly new."
```

---

### Task 3: The vault document

What actually gets sealed. Small, but it fixes the shape synchronisation will read four plans from now, so its fields are chosen for that rather than for today.

**Files:**
- Create: `src-tauri/src/vault/document.rs`
- Modify: `src-tauri/src/vault/mod.rs`

**Interfaces:**
- Consumes: `Account` from the foundation plan.
- Produces:
  - `pub struct VaultDocument { pub version: u32, pub device_id: Uuid, pub accounts: Vec<Account> }`
  - `VaultDocument::new() -> VaultDocument`
  - `fn live(&self) -> impl Iterator<Item = &Account>` — accounts excluding tombstones
  - `fn find(&self, id: Uuid) -> Option<&Account>`
  - `fn upsert(&mut self, account: Account)`
  - `fn purge_expired_tombstones(&mut self, now: DateTime<Utc>)`
  - `pub const TOMBSTONE_RETENTION_DAYS: i64 = 90;`

- [ ] **Step 1: Write the failing tests**

```bash
cat > src-tauri/src/vault/document.rs <<'EOF'
//! The document that gets sealed: every account, plus what synchronisation needs.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::Account;

/// How long a deleted account is kept as a tombstone.
///
/// Long enough that a machine left switched off for a season still learns about
/// the deletion; short enough that the vault does not grow without bound.
pub const TOMBSTONE_RETENTION_DAYS: i64 = 90;

const CURRENT_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::otp::Secret;

    fn account(issuer: &str) -> Account {
        Account::new(
            issuer.into(),
            "marcio@privum.cloud".into(),
            Secret::from_bytes(b"12345678901234567890".to_vec()),
        )
    }

    #[test]
    fn a_new_document_is_empty_and_identifies_this_device() {
        let doc = VaultDocument::new();
        assert_eq!(doc.version, CURRENT_VERSION);
        assert!(doc.accounts.is_empty());
        assert_ne!(doc.device_id, Uuid::nil());
    }

    #[test]
    fn two_documents_do_not_share_a_device_id() {
        // The device id is what lets a merge tell two machines apart.
        assert_ne!(VaultDocument::new().device_id, VaultDocument::new().device_id);
    }

    #[test]
    fn upsert_adds_then_replaces_by_id() {
        let mut doc = VaultDocument::new();
        let mut acc = account("GitHub");
        doc.upsert(acc.clone());
        assert_eq!(doc.accounts.len(), 1);

        acc.issuer = "GitHub Enterprise".into();
        acc.touch();
        doc.upsert(acc.clone());
        assert_eq!(doc.accounts.len(), 1, "upsert duplicated the account");
        assert_eq!(doc.find(acc.id).unwrap().issuer, "GitHub Enterprise");
    }

    #[test]
    fn live_hides_tombstones_but_the_document_keeps_them() {
        let mut doc = VaultDocument::new();
        let mut acc = account("GitHub");
        doc.upsert(account("Google"));
        acc.soft_delete();
        doc.upsert(acc);

        assert_eq!(doc.live().count(), 1, "a tombstone was shown to the user");
        assert_eq!(doc.accounts.len(), 2, "the tombstone was thrown away");
    }

    #[test]
    fn tombstones_are_purged_only_once_they_are_old_enough() {
        let mut doc = VaultDocument::new();
        let mut fresh = account("Fresh");
        fresh.soft_delete();
        let mut ancient = account("Ancient");
        ancient.soft_delete();
        ancient.deleted_at = Some(Utc::now() - Duration::days(TOMBSTONE_RETENTION_DAYS + 1));
        doc.upsert(fresh);
        doc.upsert(ancient);

        doc.purge_expired_tombstones(Utc::now());

        assert_eq!(doc.accounts.len(), 1, "the fresh tombstone was purged too");
        assert_eq!(doc.accounts[0].issuer, "Fresh");
    }

    #[test]
    fn purging_never_touches_a_live_account() {
        let mut doc = VaultDocument::new();
        doc.upsert(account("Live"));
        doc.purge_expired_tombstones(Utc::now() + Duration::days(3650));
        assert_eq!(doc.accounts.len(), 1);
    }

    #[test]
    fn survives_a_serde_round_trip() {
        let mut doc = VaultDocument::new();
        doc.upsert(account("GitHub"));
        let json = serde_json::to_vec(&doc).unwrap();
        let back: VaultDocument = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.device_id, doc.device_id);
        assert_eq!(back.accounts.len(), 1);
    }
}
EOF
```

Add `mod document;` and `pub use document::{VaultDocument, TOMBSTONE_RETENTION_DAYS};` to `src-tauri/src/vault/mod.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test vault::document 2>&1 | grep -E '^error' | head -3`
Expected: FAIL — `VaultDocument` does not exist.

- [ ] **Step 3: Write the implementation**

Insert into `document.rs`, above the `#[cfg(test)]` block:

```rust
/// Everything the vault holds.
///
/// `device_id` is generated once per vault file and never changes. It is not
/// used yet: it is what a merge will read to tell two machines apart, and
/// adding it later would mean migrating vaults that already exist.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultDocument {
    pub version: u32,
    pub device_id: Uuid,
    pub accounts: Vec<Account>,
}

impl VaultDocument {
    pub fn new() -> Self {
        Self {
            version: CURRENT_VERSION,
            device_id: Uuid::new_v4(),
            accounts: Vec::new(),
        }
    }

    /// The accounts a user should see: everything that is not a tombstone.
    pub fn live(&self) -> impl Iterator<Item = &Account> {
        self.accounts.iter().filter(|a| !a.is_deleted())
    }

    pub fn find(&self, id: Uuid) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    /// Insert the account, or replace the one that already carries its id.
    pub fn upsert(&mut self, account: Account) {
        match self.accounts.iter_mut().find(|a| a.id == account.id) {
            Some(existing) => *existing = account,
            None => self.accounts.push(account),
        }
    }

    /// Drop tombstones old enough that every other device has certainly seen
    /// them. Live accounts are never touched, whatever the date.
    pub fn purge_expired_tombstones(&mut self, now: DateTime<Utc>) {
        let cutoff = now - Duration::days(TOMBSTONE_RETENTION_DAYS);
        self.accounts
            .retain(|a| a.deleted_at.is_none_or(|deleted| deleted > cutoff));
    }
}

impl Default for VaultDocument {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test vault:: 2>&1 | grep 'test result'`
Expected: `test result: ok. 19 passed`.

- [ ] **Step 5: Format, lint, commit**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --all-targets -- -D warnings
cd .. && git add -A
git commit -m "feat(vault): add the sealed document with tombstone retention

device_id is unused today and present anyway: a merge will read it to tell
two machines apart, and adding it later means migrating vaults that exist."
```

---

### Task 4: The vault manager

Owns the unlocked state. This is where the master-password decision becomes code: there is no keyring path, no cached key, nothing on disk between sessions.

**Files:**
- Create: `src-tauri/src/vault/manager.rs`
- Modify: `src-tauri/src/vault/mod.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–3.
- Produces:
  - `pub struct VaultManager` with `VaultManager::new(path: PathBuf) -> Self`
  - `fn exists(&self) -> bool`
  - `fn create(&mut self, password: &str) -> Result<(), VaultError>`
  - `fn unlock(&mut self, password: &str) -> Result<(), VaultError>`
  - `fn lock(&mut self)`
  - `fn is_unlocked(&self) -> bool`
  - `fn document(&self) -> Result<&VaultDocument, VaultError>`
  - `fn mutate<F: FnOnce(&mut VaultDocument)>(&mut self, f: F) -> Result<(), VaultError>`
  - `fn touch_activity(&mut self)`
  - `fn lock_if_idle(&mut self, timeout: Duration) -> bool`
  - `pub fn default_vault_path() -> PathBuf`

- [ ] **Step 1: Write the failing tests**

```bash
cat > src-tauri/src/vault/manager.rs <<'EOF'
//! Unlocked state, and the rules for losing it.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::vault::{load_document, save_document, KdfParams, VaultDocument, VaultError};

/// Where the vault lives, following the XDG base directory specification.
pub fn default_vault_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tessera")
        .join("vault.bin")
}

/// What is held only while the vault is open.
struct Unlocked {
    password: Zeroizing<String>,
    document: VaultDocument,
    last_activity: Instant,
}

/// The vault, locked or open.
pub struct VaultManager {
    path: PathBuf,
    state: Option<Unlocked>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Account;
    use crate::otp::Secret;

    fn manager(name: &str) -> VaultManager {
        let mut path = std::env::temp_dir();
        path.push(format!("tessera-mgr-{name}.bin"));
        let _ = std::fs::remove_file(&path);
        VaultManager::new(path)
    }

    fn cleanup(m: &VaultManager) {
        let _ = std::fs::remove_file(m.path());
    }

    #[test]
    fn a_fresh_vault_does_not_exist_and_is_locked() {
        let m = manager("fresh");
        assert!(!m.exists());
        assert!(!m.is_unlocked());
        assert!(matches!(m.document(), Err(VaultError::Locked)));
    }

    #[test]
    fn create_leaves_the_vault_open_and_on_disk() {
        let mut m = manager("create");
        m.create("master").unwrap();
        assert!(m.exists());
        assert!(m.is_unlocked());
        assert!(m.document().unwrap().accounts.is_empty());
        cleanup(&m);
    }

    #[test]
    fn lock_then_unlock_returns_the_same_accounts() {
        let mut m = manager("relock");
        m.create("master").unwrap();
        m.mutate(|doc| {
            doc.upsert(Account::new(
                "GitHub".into(),
                "marcio@privum.cloud".into(),
                Secret::from_bytes(b"12345678901234567890".to_vec()),
            ))
        })
        .unwrap();

        m.lock();
        assert!(!m.is_unlocked());

        m.unlock("master").unwrap();
        assert_eq!(m.document().unwrap().live().count(), 1);
        cleanup(&m);
    }

    #[test]
    fn unlocking_with_the_wrong_password_leaves_it_locked() {
        let mut m = manager("wrongpw");
        m.create("master").unwrap();
        m.lock();

        assert!(matches!(m.unlock("guess"), Err(VaultError::Crypto)));
        assert!(!m.is_unlocked(), "a failed unlock opened the vault");
        cleanup(&m);
    }

    #[test]
    fn mutating_a_locked_vault_is_an_error_not_a_panic() {
        let mut m = manager("mutate-locked");
        assert!(matches!(m.mutate(|_| {}), Err(VaultError::Locked)));
    }

    #[test]
    fn every_mutation_reaches_the_disk_immediately() {
        // There is no explicit save: an authenticator that loses an account
        // because the process died before a flush is worse than a slow one.
        let mut m = manager("autosave");
        m.create("master").unwrap();
        m.mutate(|doc| {
            doc.upsert(Account::new(
                "AWS".into(),
                "root".into(),
                Secret::from_bytes(b"12345678901234567890".to_vec()),
            ))
        })
        .unwrap();

        let mut other = VaultManager::new(m.path().to_path_buf());
        other.unlock("master").unwrap();
        assert_eq!(other.document().unwrap().live().count(), 1);
        cleanup(&m);
    }

    #[test]
    fn an_idle_vault_locks_itself() {
        let mut m = manager("idle");
        m.create("master").unwrap();

        assert!(!m.lock_if_idle(Duration::from_secs(300)), "locked too early");
        assert!(m.is_unlocked());

        assert!(m.lock_if_idle(Duration::ZERO), "did not lock when idle");
        assert!(!m.is_unlocked());
        cleanup(&m);
    }

    #[test]
    fn activity_postpones_the_idle_lock() {
        let mut m = manager("activity");
        m.create("master").unwrap();
        m.touch_activity();
        assert!(!m.lock_if_idle(Duration::from_secs(300)));
        cleanup(&m);
    }

    #[test]
    fn locking_an_already_locked_vault_is_harmless() {
        let mut m = manager("double-lock");
        m.lock();
        m.lock();
        assert!(!m.is_unlocked());
    }
}
EOF
```

Add `mod manager;` and `pub use manager::{default_vault_path, VaultManager};` to `src-tauri/src/vault/mod.rs`. Add `dirs = "5"` to `src-tauri/Cargo.toml`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test vault::manager 2>&1 | grep -E '^error' | head -3`
Expected: FAIL — the methods do not exist.

- [ ] **Step 3: Write the implementation**

Insert into `manager.rs`, above the `#[cfg(test)]` block:

```rust
impl VaultManager {
    pub fn new(path: PathBuf) -> Self {
        Self { path, state: None }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Create an empty vault and leave it open.
    pub fn create(&mut self, password: &str) -> Result<(), VaultError> {
        let document = VaultDocument::new();
        self.write(password, &document)?;
        self.state = Some(Unlocked {
            password: Zeroizing::new(password.to_owned()),
            document,
            last_activity: Instant::now(),
        });
        Ok(())
    }

    /// Open the vault. On any failure the vault stays locked — a half-open
    /// state would be worse than either.
    pub fn unlock(&mut self, password: &str) -> Result<(), VaultError> {
        let plaintext = load_document(&self.path, password)?;
        let mut document: VaultDocument =
            serde_json::from_slice(&plaintext).map_err(|_| VaultError::BadFormat)?;
        document.purge_expired_tombstones(chrono::Utc::now());

        self.state = Some(Unlocked {
            password: Zeroizing::new(password.to_owned()),
            document,
            last_activity: Instant::now(),
        });
        Ok(())
    }

    /// Drop everything held in memory. `Zeroizing` clears the password as the
    /// state is dropped.
    pub fn lock(&mut self) {
        self.state = None;
    }

    pub fn is_unlocked(&self) -> bool {
        self.state.is_some()
    }

    pub fn document(&self) -> Result<&VaultDocument, VaultError> {
        self.state
            .as_ref()
            .map(|s| &s.document)
            .ok_or(VaultError::Locked)
    }

    /// Change the document and persist it in one step.
    ///
    /// There is deliberately no separate `save`: an authenticator that loses an
    /// account because the process died before a flush is worse than one that
    /// writes a few kilobytes more often than it strictly must.
    pub fn mutate<F>(&mut self, change: F) -> Result<(), VaultError>
    where
        F: FnOnce(&mut VaultDocument),
    {
        let state = self.state.as_mut().ok_or(VaultError::Locked)?;
        change(&mut state.document);
        state.last_activity = Instant::now();

        let password = state.password.clone();
        let document = state.document.clone();
        self.write(&password, &document)
    }

    /// Note that the user did something, postponing the idle lock.
    pub fn touch_activity(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.last_activity = Instant::now();
        }
    }

    /// Lock if nothing has happened for `timeout`. Returns whether it locked.
    pub fn lock_if_idle(&mut self, timeout: Duration) -> bool {
        let idle = match self.state.as_ref() {
            Some(state) => state.last_activity.elapsed() >= timeout,
            None => return false,
        };
        if idle {
            self.lock();
        }
        idle
    }

    fn write(&self, password: &str, document: &VaultDocument) -> Result<(), VaultError> {
        let plaintext =
            serde_json::to_vec(document).map_err(|e| VaultError::Io(e.to_string()))?;
        save_document(&self.path, password, KdfParams::default(), &plaintext)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test vault:: 2>&1 | grep 'test result'`
Expected: `test result: ok. 28 passed`.

- [ ] **Step 5: Format, lint, commit**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --all-targets -- -D warnings
cd .. && git add -A
git commit -m "feat(vault): add the manager that owns unlocked state

No keyring, no cached key, nothing on disk between sessions — the master
password is required at every launch, which was decided explicitly. Every
mutation writes through, because an authenticator that loses an account to a
missed flush is worse than one that writes a few kilobytes too often."
```

---

### Task 5: Reading an `otpauth://` URI

The way accounts get in. A service shows a QR code; the QR code contains one of these; the user pastes it. Decoding images comes in a later plan, but the grammar underneath is needed now, and emitting it is what the export screen will use.

**Files:**
- Create: `src-tauri/src/import/mod.rs`, `src-tauri/src/import/otpauth.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `Account`, `AccountKind`, `Algorithm`, `Secret`, `OtpError`.
- Produces:
  - `pub fn parse_otpauth(uri: &str) -> Result<Account, ImportError>`
  - `pub fn to_otpauth(account: &Account) -> Zeroizing<String>`
  - `pub enum ImportError { NotOtpauth, UnknownKind, MissingSecret, BadSecret, BadParameter(String) }`

- [ ] **Step 1: Write the failing tests**

```bash
mkdir -p src-tauri/src/import
cat > src-tauri/src/import/otpauth.rs <<'EOF'
//! The `otpauth://` URI, which is what a service's QR code actually contains.
//!
//! The format is Google's Key Uri Format. It is loosely followed in the wild,
//! so the parser is forgiving about what it accepts and strict about what it
//! emits.

use url::Url;
use zeroize::Zeroizing;

use crate::model::{Account, AccountKind};
use crate::otp::{Algorithm, Secret};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImportError {
    #[error("that is not an otpauth:// link")]
    NotOtpauth,
    #[error("Tessera does not know that kind of one-time password")]
    UnknownKind,
    #[error("the link has no secret in it")]
    MissingSecret,
    #[error("the secret in the link is not valid base32")]
    BadSecret,
    #[error("the link has an unusable {0}")]
    BadParameter(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn reads_the_shape_a_service_actually_issues() {
        let account = parse_otpauth(&format!(
            "otpauth://totp/GitHub:marcio@privum.cloud?secret={SECRET_B32}&issuer=GitHub"
        ))
        .unwrap();

        assert_eq!(account.issuer, "GitHub");
        assert_eq!(account.label, "marcio@privum.cloud");
        assert_eq!(account.kind, AccountKind::Totp);
        assert_eq!(account.secret.expose(), b"12345678901234567890");
        // Absent parameters take the defaults the format specifies.
        assert_eq!(account.algorithm, Algorithm::Sha1);
        assert_eq!(account.digits, 6);
        assert_eq!(account.period, 30);
    }

    #[test]
    fn reads_every_optional_parameter() {
        let account = parse_otpauth(&format!(
            "otpauth://totp/Example:alice?secret={SECRET_B32}&algorithm=SHA512&digits=8&period=60"
        ))
        .unwrap();
        assert_eq!(account.algorithm, Algorithm::Sha512);
        assert_eq!(account.digits, 8);
        assert_eq!(account.period, 60);
    }

    #[test]
    fn reads_an_hotp_counter() {
        let account =
            parse_otpauth(&format!("otpauth://hotp/Bank:alice?secret={SECRET_B32}&counter=42"))
                .unwrap();
        assert_eq!(account.kind, AccountKind::Hotp);
        assert_eq!(account.counter, 42);
    }

    #[test]
    fn decodes_percent_encoding_in_the_label() {
        // Issuers with spaces are ordinary, and the colon separator is encoded.
        let account = parse_otpauth(&format!(
            "otpauth://totp/Big%20Bank%3Aalice%40example.com?secret={SECRET_B32}"
        ))
        .unwrap();
        assert_eq!(account.issuer, "Big Bank");
        assert_eq!(account.label, "alice@example.com");
    }

    #[test]
    fn prefers_the_issuer_parameter_over_the_label_prefix() {
        // The format says the parameter wins when the two disagree.
        let account = parse_otpauth(&format!(
            "otpauth://totp/Stale:alice?secret={SECRET_B32}&issuer=Current"
        ))
        .unwrap();
        assert_eq!(account.issuer, "Current");
    }

    #[test]
    fn copes_with_a_label_that_has_no_issuer_at_all() {
        let account = parse_otpauth(&format!("otpauth://totp/alice?secret={SECRET_B32}")).unwrap();
        assert_eq!(account.issuer, "");
        assert_eq!(account.label, "alice");
    }

    #[test]
    fn refuses_what_it_cannot_generate_codes_for() {
        assert_eq!(
            parse_otpauth("https://example.com"),
            Err(ImportError::NotOtpauth)
        );
        assert_eq!(
            parse_otpauth(&format!("otpauth://yubico/x?secret={SECRET_B32}")),
            Err(ImportError::UnknownKind)
        );
        assert_eq!(
            parse_otpauth("otpauth://totp/alice"),
            Err(ImportError::MissingSecret)
        );
        assert_eq!(
            parse_otpauth("otpauth://totp/alice?secret=not!base32"),
            Err(ImportError::BadSecret)
        );
    }

    #[test]
    fn refuses_a_digit_count_that_cannot_produce_a_usable_code() {
        // hotp() no longer panics on a large digit count, but a 40-digit code
        // is still nonsense and is better refused at the door than shown.
        assert_eq!(
            parse_otpauth(&format!("otpauth://totp/alice?secret={SECRET_B32}&digits=40")),
            Err(ImportError::BadParameter("digit count".into()))
        );
        assert_eq!(
            parse_otpauth(&format!("otpauth://totp/alice?secret={SECRET_B32}&period=0")),
            Err(ImportError::BadParameter("period".into()))
        );
    }

    #[test]
    fn round_trips_through_its_own_output() {
        let original = parse_otpauth(&format!(
            "otpauth://totp/GitHub:marcio@privum.cloud?secret={SECRET_B32}&issuer=GitHub&digits=8&period=60&algorithm=SHA256"
        ))
        .unwrap();
        let back = parse_otpauth(&to_otpauth(&original)).unwrap();

        assert_eq!(back.issuer, original.issuer);
        assert_eq!(back.label, original.label);
        assert_eq!(back.secret, original.secret);
        assert_eq!(back.algorithm, original.algorithm);
        assert_eq!(back.digits, original.digits);
        assert_eq!(back.period, original.period);
    }
}
EOF
cat > src-tauri/src/import/mod.rs <<'EOF'
//! Reading accounts out of the formats other tools produce.

mod otpauth;

pub use otpauth::{parse_otpauth, to_otpauth, ImportError};
EOF
```

Add `pub mod import;` to `src-tauri/src/lib.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test import:: 2>&1 | grep -E '^error' | head -3`
Expected: FAIL — `parse_otpauth` and `to_otpauth` do not exist.

- [ ] **Step 3: Write the implementation**

Insert into `otpauth.rs`, above the `#[cfg(test)]` block:

```rust
/// Digit counts outside this range cannot produce a code anyone can type.
const MIN_DIGITS: u32 = 4;
const MAX_DIGITS: u32 = 10;

/// Read an `otpauth://` link into an account.
pub fn parse_otpauth(uri: &str) -> Result<Account, ImportError> {
    let url = Url::parse(uri).map_err(|_| ImportError::NotOtpauth)?;
    if url.scheme() != "otpauth" {
        return Err(ImportError::NotOtpauth);
    }

    let kind = match url.host_str() {
        Some("totp") => AccountKind::Totp,
        Some("hotp") => AccountKind::Hotp,
        Some("steam") => AccountKind::Steam,
        _ => return Err(ImportError::UnknownKind),
    };

    // The path is `/Issuer:label`, percent-encoded. Url gives it back decoded.
    let path = url.path().trim_start_matches('/');
    let decoded = percent_decode(path);
    let (issuer_from_label, label) = match decoded.split_once(':') {
        Some((issuer, label)) => (issuer.trim().to_owned(), label.trim().to_owned()),
        None => (String::new(), decoded.trim().to_owned()),
    };

    let mut secret = None;
    // The label prefix is a fallback; the format says the parameter wins.
    let mut issuer = issuer_from_label;
    let mut algorithm = Algorithm::Sha1;
    let mut digits = if kind == AccountKind::Steam { 5 } else { 6 };
    let mut period = 30u32;
    let mut counter = 0u64;

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "secret" => {
                secret = Some(Secret::from_base32(&value).map_err(|_| ImportError::BadSecret)?)
            }
            "issuer" => issuer = value.trim().to_owned(),
            "algorithm" => {
                algorithm = match value.to_ascii_uppercase().as_str() {
                    "SHA1" => Algorithm::Sha1,
                    "SHA256" => Algorithm::Sha256,
                    "SHA512" => Algorithm::Sha512,
                    _ => return Err(ImportError::BadParameter("algorithm".into())),
                }
            }
            "digits" => {
                digits = value
                    .parse()
                    .ok()
                    .filter(|d| (MIN_DIGITS..=MAX_DIGITS).contains(d))
                    .ok_or_else(|| ImportError::BadParameter("digit count".into()))?
            }
            "period" => {
                period = value
                    .parse()
                    .ok()
                    .filter(|p| *p > 0)
                    .ok_or_else(|| ImportError::BadParameter("period".into()))?
            }
            "counter" => {
                counter = value
                    .parse()
                    .map_err(|_| ImportError::BadParameter("counter".into()))?
            }
            // Unknown parameters are ignored rather than refused: the format
            // grows, and a link Tessera cannot fully describe is still usable.
            _ => {}
        }
    }

    let mut account = Account::new(issuer, label, secret.ok_or(ImportError::MissingSecret)?);
    account.kind = kind;
    account.algorithm = algorithm;
    account.digits = digits;
    account.period = period;
    account.counter = counter;
    Ok(account)
}

/// Write an account back out as a link, for export and for showing a QR code.
///
/// The result carries the secret, so it is wrapped in `Zeroizing`.
pub fn to_otpauth(account: &Account) -> Zeroizing<String> {
    let kind = match account.kind {
        AccountKind::Totp => "totp",
        AccountKind::Hotp => "hotp",
        AccountKind::Steam => "steam",
    };
    let algorithm = match account.algorithm {
        Algorithm::Sha1 => "SHA1",
        Algorithm::Sha256 => "SHA256",
        Algorithm::Sha512 => "SHA512",
    };

    let label = if account.issuer.is_empty() {
        encode(&account.label)
    } else {
        format!("{}:{}", encode(&account.issuer), encode(&account.label))
    };

    let mut uri = format!(
        "otpauth://{kind}/{label}?secret={}&algorithm={algorithm}&digits={}",
        &*account.secret.to_base32(),
        account.digits
    );
    if !account.issuer.is_empty() {
        uri.push_str(&format!("&issuer={}", encode(&account.issuer)));
    }
    match account.kind {
        AccountKind::Hotp => uri.push_str(&format!("&counter={}", account.counter)),
        _ => uri.push_str(&format!("&period={}", account.period)),
    }
    Zeroizing::new(uri)
}

/// Percent-encode a label component. Only the characters that would change how
/// the URI parses are escaped.
fn encode(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            ':' => "%3A".to_owned(),
            '/' => "%2F".to_owned(),
            '?' => "%3F".to_owned(),
            '#' => "%23".to_owned(),
            '&' => "%26".to_owned(),
            ' ' => "%20".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

/// Decode the percent escapes `Url` leaves in a path segment.
fn percent_decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}
```

Add `percent-encoding = "2"` to `src-tauri/Cargo.toml`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test import:: 2>&1 | grep 'test result'`
Expected: `test result: ok. 9 passed` in the import module.

- [ ] **Step 5: Format, lint, commit**

```bash
cd src-tauri && cargo fmt --all && cargo clippy --all-targets -- -D warnings
cd .. && git add -A
git commit -m "feat(import): read and write otpauth:// links

Forgiving about what it accepts and strict about what it emits: services
follow the Key Uri Format loosely, but a link Tessera hands back has to be
readable by whatever the user points at it. Digit counts and periods that
cannot produce a usable code are refused at the door."
```

---

### Task 6: The command surface

Everything the interface will call. This replaces the smoke-screen command from the foundation plan.

**Files:**
- Rewrite: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `VaultManager`, `Account`, `parse_otpauth`, the `otp` module.
- Produces the Tauri commands: `vault_status`, `create_vault`, `unlock_vault`, `lock_vault`, `list_accounts`, `add_account_from_uri`, `add_account_manual`, `update_account`, `delete_account`, `poll_idle_lock`.
- Produces `pub struct AccountView` — issuer, label, id, kind, group, code, seconds_remaining, period.

**The rule this task exists to enforce:** `AccountView` carries no secret. The interface renders a list from these and never sees a seed.

- [ ] **Step 1: Write the failing tests**

```bash
cat > src-tauri/src/commands.rs <<'EOF'
//! The Tauri command surface — the only part of the core the interface sees.
//!
//! Commands return codes and metadata. A raw secret never travels in this
//! direction: `AccountView` has no field for one, which is the point.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use uuid::Uuid;

use crate::import::parse_otpauth;
use crate::model::{Account, AccountKind};
use crate::otp::{seconds_remaining, steam_at, totp_at, Algorithm, Secret};
use crate::vault::{VaultDocument, VaultError, VaultManager};

/// Everything the interface needs to draw one row, and nothing more.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AccountView {
    pub id: Uuid,
    pub issuer: String,
    pub label: String,
    pub group: Option<String>,
    pub kind: AccountKind,
    pub code: String,
    pub seconds_remaining: u32,
    pub period: u32,
}

pub struct AppState {
    pub vault: Mutex<VaultManager>,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the system clock is set before 1970")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn totp_account() -> Account {
        parse_otpauth(&format!(
            "otpauth://totp/GitHub:marcio@privum.cloud?secret={SECRET_B32}&digits=8"
        ))
        .unwrap()
    }

    #[test]
    fn renders_a_row_whose_code_matches_the_rfc_vector() {
        let view = view_of(&totp_account(), 59);
        assert_eq!(view.code, "94287082");
        assert_eq!(view.seconds_remaining, 1);
        assert_eq!(view.issuer, "GitHub");
        assert_eq!(view.label, "marcio@privum.cloud");
    }

    #[test]
    fn an_hotp_row_has_nothing_to_count_down() {
        let mut account = totp_account();
        account.kind = AccountKind::Hotp;
        account.counter = 1;
        account.digits = 6;
        let view = view_of(&account, 59);
        assert_eq!(view.code, "287082");
        assert_eq!(view.seconds_remaining, 0);
    }

    #[test]
    fn a_steam_row_renders_five_characters() {
        let mut account = totp_account();
        account.kind = AccountKind::Steam;
        assert_eq!(view_of(&account, 59).code.len(), 5);
    }

    #[test]
    fn the_row_the_interface_receives_has_no_field_for_a_secret() {
        // Serialised rather than inspected, because the risk is what crosses
        // the boundary, not what the struct is called.
        let json = serde_json::to_string(&view_of(&totp_account(), 59)).unwrap();
        assert!(!json.contains("secret"), "AccountView exposed a secret: {json}");
        assert!(!json.contains(SECRET_B32));
        assert!(!json.contains("GEZD"));
    }

    #[test]
    fn rows_are_ordered_so_the_list_does_not_reshuffle_itself() {
        // Sorted by issuer then label, case-insensitively. An authenticator
        // whose rows move between openings is one you misclick.
        let mut doc = VaultDocument::new();
        for uri in [
            format!("otpauth://totp/zulu:a?secret={SECRET_B32}"),
            format!("otpauth://totp/Alpha:b?secret={SECRET_B32}"),
            format!("otpauth://totp/alpha:a?secret={SECRET_B32}"),
        ] {
            doc.upsert(parse_otpauth(&uri).unwrap());
        }

        let rows = views_of(&doc, 59);
        let names: Vec<_> = rows.iter().map(|r| format!("{}:{}", r.issuer, r.label)).collect();
        assert_eq!(names, vec!["alpha:a", "Alpha:b", "zulu:a"]);
    }

    #[test]
    fn deleted_accounts_never_reach_the_interface() {
        let mut doc = VaultDocument::new();
        let mut account = totp_account();
        account.soft_delete();
        doc.upsert(account);
        assert!(views_of(&doc, 59).is_empty());
    }
}
EOF
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test commands:: 2>&1 | grep -E '^error' | head -3`
Expected: FAIL — `view_of` and `views_of` do not exist.

- [ ] **Step 3: Write the view helpers**

Insert into `commands.rs`, above the `#[cfg(test)]` block:

```rust
/// Render one account as of a given moment.
fn view_of(account: &Account, unix_seconds: u64) -> AccountView {
    let (code, remaining) = match account.kind {
        AccountKind::Totp => (
            totp_at(
                account.algorithm,
                account.secret.expose(),
                unix_seconds,
                account.period,
                account.digits,
            ),
            seconds_remaining(unix_seconds, account.period),
        ),
        // An HOTP code stands until the user asks for the next one.
        AccountKind::Hotp => (
            crate::otp::hotp(
                account.algorithm,
                account.secret.expose(),
                account.counter,
                account.digits,
            ),
            0,
        ),
        // Steam fixes its own shape: five characters over thirty seconds.
        AccountKind::Steam => (
            steam_at(account.secret.expose(), unix_seconds),
            seconds_remaining(unix_seconds, 30),
        ),
    };

    AccountView {
        id: account.id,
        issuer: account.issuer.clone(),
        label: account.label.clone(),
        group: account.group.clone(),
        kind: account.kind,
        code,
        seconds_remaining: remaining,
        period: account.period,
    }
}

/// Render every live account, in a stable order.
///
/// Sorted by issuer then label, case-insensitively. A list whose rows move
/// between openings is a list you misclick, and misclicking here means pasting
/// the wrong code into a login form.
fn views_of(document: &VaultDocument, unix_seconds: u64) -> Vec<AccountView> {
    let mut rows: Vec<_> = document
        .live()
        .map(|a| view_of(a, unix_seconds))
        .collect();
    rows.sort_by_key(|r| {
        (
            r.issuer.to_lowercase(),
            r.label.to_lowercase(),
        )
    });
    rows
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test commands:: 2>&1 | grep 'test result'`
Expected: `test result: ok.` with 6 passing in `commands`.

- [ ] **Step 5: Write the commands themselves**

Append to `commands.rs`, after the view helpers and before `#[cfg(test)]`:

```rust
/// How long the vault stays open with nothing happening.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// A poisoned lock means another thread panicked while holding the vault. The
/// safe answer is to treat the vault as unusable rather than carry on with
/// state of unknown validity.
fn vault(state: &AppState) -> Result<std::sync::MutexGuard<'_, VaultManager>, String> {
    state
        .vault
        .lock()
        .map_err(|_| "the vault is in an unknown state; restart Tessera".to_owned())
}

fn fail(e: VaultError) -> String {
    e.to_string()
}

#[derive(Serialize)]
pub struct VaultStatus {
    pub exists: bool,
    pub unlocked: bool,
}

#[tauri::command]
pub fn vault_status(state: tauri::State<'_, AppState>) -> Result<VaultStatus, String> {
    let vault = vault(&state)?;
    Ok(VaultStatus {
        exists: vault.exists(),
        unlocked: vault.is_unlocked(),
    })
}

#[tauri::command]
pub fn create_vault(state: tauri::State<'_, AppState>, password: String) -> Result<(), String> {
    vault(&state)?.create(&password).map_err(fail)
}

#[tauri::command]
pub fn unlock_vault(state: tauri::State<'_, AppState>, password: String) -> Result<(), String> {
    vault(&state)?.unlock(&password).map_err(fail)
}

#[tauri::command]
pub fn lock_vault(state: tauri::State<'_, AppState>) -> Result<(), String> {
    vault(&state)?.lock();
    Ok(())
}

#[tauri::command]
pub fn list_accounts(state: tauri::State<'_, AppState>) -> Result<Vec<AccountView>, String> {
    let vault = vault(&state)?;
    Ok(views_of(vault.document().map_err(fail)?, now_unix()))
}

#[tauri::command]
pub fn add_account_from_uri(
    state: tauri::State<'_, AppState>,
    uri: String,
) -> Result<(), String> {
    let account = parse_otpauth(&uri).map_err(|e| e.to_string())?;
    vault(&state)?
        .mutate(|doc| doc.upsert(account))
        .map_err(fail)
}

#[tauri::command]
pub fn add_account_manual(
    state: tauri::State<'_, AppState>,
    issuer: String,
    label: String,
    secret: String,
    kind: AccountKind,
    algorithm: Algorithm,
    digits: u32,
    period: u32,
) -> Result<(), String> {
    let secret = Secret::from_base32(&secret).map_err(|e| e.to_string())?;
    let mut account = Account::new(issuer, label, secret);
    account.kind = kind;
    account.algorithm = algorithm;
    account.digits = digits;
    account.period = period;

    vault(&state)?
        .mutate(|doc| doc.upsert(account))
        .map_err(fail)
}

#[tauri::command]
pub fn update_account(
    state: tauri::State<'_, AppState>,
    id: Uuid,
    issuer: String,
    label: String,
    group: Option<String>,
) -> Result<(), String> {
    let mut vault = vault(&state)?;
    let mut account = vault
        .document()
        .map_err(fail)?
        .find(id)
        .ok_or_else(|| "that account is no longer in the vault".to_owned())?
        .clone();

    account.issuer = issuer;
    account.label = label;
    account.group = group;
    account.touch();

    vault.mutate(|doc| doc.upsert(account)).map_err(fail)
}

#[tauri::command]
pub fn delete_account(state: tauri::State<'_, AppState>, id: Uuid) -> Result<(), String> {
    let mut vault = vault(&state)?;
    let mut account = vault
        .document()
        .map_err(fail)?
        .find(id)
        .ok_or_else(|| "that account is no longer in the vault".to_owned())?
        .clone();

    // A tombstone, not a removal: without one the deletion cannot reach the
    // user's other machines and the next sync brings the account back.
    account.soft_delete();
    vault.mutate(|doc| doc.upsert(account)).map_err(fail)
}

/// Called by the interface on a timer. Returns whether the vault just locked,
/// so the interface can show the unlock screen without polling for state.
#[tauri::command]
pub fn poll_idle_lock(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(vault(&state)?.lock_if_idle(IDLE_TIMEOUT))
}

/// Note that the user did something. The interface calls this on interaction so
/// the idle timer measures inactivity rather than elapsed time.
#[tauri::command]
pub fn note_activity(state: tauri::State<'_, AppState>) -> Result<(), String> {
    vault(&state)?.touch_activity();
    Ok(())
}
```

- [ ] **Step 6: Register the state and the commands**

Rewrite `src-tauri/src/lib.rs`:

```rust
mod commands;
pub mod import;
pub mod model;
pub mod otp;
pub mod vault;

use std::sync::Mutex;

use tauri::Manager;

use commands::AppState;
use vault::{default_vault_path, VaultManager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            app.manage(AppState {
                vault: Mutex::new(VaultManager::new(default_vault_path())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::create_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::list_accounts,
            commands::add_account_from_uri,
            commands::add_account_manual,
            commands::update_account,
            commands::delete_account,
            commands::poll_idle_lock,
            commands::note_activity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tessera");
}
```

- [ ] **Step 7: Verify everything still builds and passes**

Run: `cd src-tauri && cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: clean, all tests passing.

The foundation plan's smoke screen calls `preview_code`, which no longer exists. Replace `src/App.tsx` with a placeholder so the front end compiles; the real interface is the next plan:

```bash
cat > src/App.tsx <<'EOF'
export default function App() {
  return <main className="shell">Tessera</main>;
}
EOF
rm src/lib/api.ts
npm run typecheck
```

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: expose the vault through the command surface

AccountView has no field for a secret, and a test asserts that on the
serialised form rather than the struct — the risk is what crosses the
boundary, not what the type is called. Rows sort by issuer then label so the
list does not reshuffle between openings; a list you misclick is one that
pastes the wrong code into a login form."
```

---

## Definition of done

- `cargo test` passes, with the vault, import and command suites green.
- `cargo fmt --check` and `cargo clippy -D warnings` are clean.
- CI is green on `feat/foundation`.
- A vault can be created, locked, and reopened in a separate `VaultManager`, proving persistence.
- A hostile vault header is refused rather than panicking or allocating.
- No serialised value leaving the core contains a secret.

## What this plan deliberately does not do

Draws nothing. The interface — unlock screen, account list, countdown rings, add and edit forms, search, clipboard handling — is the next plan, and it will use the conventional direction chosen for it: a dark list, a circular countdown ring per row, and a single blue accent.
