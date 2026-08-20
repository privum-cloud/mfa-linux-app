//! Unlocked state, and the rules for losing it.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::vault::{load_document, save_document, KdfParams, Settings, VaultDocument, VaultError};

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

    /// How long this vault stays open with nothing happening.
    ///
    /// Locked vaults report the default, because there is nothing to lock and
    /// the caller only needs a number to sleep on.
    pub fn idle_timeout(&self) -> Duration {
        let secs = self
            .state
            .as_ref()
            .map(|s| s.document.settings.validated().idle_timeout_secs)
            .unwrap_or(Settings::default().idle_timeout_secs);
        Duration::from_secs(u64::from(secs))
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
        let plaintext = serde_json::to_vec(document).map_err(|e| VaultError::Io(e.to_string()))?;
        save_document(&self.path, password, KdfParams::default(), &plaintext)
    }
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

    fn sample() -> Account {
        Account::new(
            "GitHub".into(),
            "you@example.com".into(),
            Secret::from_bytes(b"12345678901234567890".to_vec()),
        )
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
        m.mutate(|doc| doc.upsert(sample())).unwrap();

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
        m.mutate(|doc| doc.upsert(sample())).unwrap();

        let mut other = VaultManager::new(m.path().to_path_buf());
        other.unlock("master").unwrap();
        assert_eq!(other.document().unwrap().live().count(), 1);
        cleanup(&m);
    }

    #[test]
    fn an_idle_vault_locks_itself() {
        let mut m = manager("idle");
        m.create("master").unwrap();

        assert!(
            !m.lock_if_idle(Duration::from_secs(300)),
            "locked too early"
        );
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
