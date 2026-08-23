//! Unlocked state, and the rules for losing it.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use zeroize::Zeroizing;

use crate::sync::merge;
use crate::vault::{
    load_document, save_document_to, KdfParams, Location, Settings, VaultDocument, VaultError,
};

/// Something different for every manager built, so two of them never write
/// through the same temporary file.
fn instance_suffix() -> String {
    let mut bytes = [0u8; 4];
    let _ = getrandom::getrandom(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where the vault lives.
///
/// On Linux this follows the XDG base directory specification. On Windows it is
/// deliberately `%LOCALAPPDATA%` rather than the roaming `%APPDATA%`: a roaming
/// profile is copied to a domain server at sign-out, and a file holding second
/// factors has no business travelling to one without the user asking.
pub fn default_vault_path() -> PathBuf {
    #[cfg(windows)]
    let base = dirs::data_local_dir();
    #[cfg(not(windows))]
    let base = dirs::data_dir();

    base.unwrap_or_else(|| PathBuf::from("."))
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
    /// Keeps two writers off one temporary file. Random per manager, so it is
    /// distinct across machines, across processes, and within a process.
    writer_id: String,
    /// Modified time and length of the vault the last time this manager read or
    /// wrote it. Comparing a `stat` against this is how an outside change is
    /// noticed without paying for another key derivation.
    last_seen: Option<(SystemTime, u64)>,
}

impl VaultManager {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: None,
            // Machine identity plus something fresh per manager. The machine
            // part says which computer left a stray temporary behind; the fresh
            // part is what keeps two windows on one machine apart, which the
            // machine id alone cannot do.
            writer_id: format!("{}-{}", Location::load().device_id(), instance_suffix()),
            last_seen: None,
        }
    }

    /// The temporary file this manager writes through.
    ///
    /// Named per writer: a shared `vault.bin.tmp` is one file every machine
    /// would use, and two saves at once interleave into a rename that produces
    /// neither document.
    pub fn temp_path(&self) -> PathBuf {
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "vault.bin".to_owned());
        self.path
            .with_file_name(format!("{name}.{}.tmp", self.writer_id))
    }

    /// Modified time and length of the vault file, or None if it is not there.
    fn stamp(&self) -> Option<(SystemTime, u64)> {
        let meta = std::fs::metadata(&self.path).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    /// Has anything written to the vault since this manager last touched it?
    fn changed_underneath(&self) -> bool {
        self.stamp() != self.last_seen
    }

    /// Take in whatever another machine wrote, if anything.
    ///
    /// Returns whether the document changed. Cheap when nothing happened: the
    /// comparison is a `stat`, not another key derivation.
    pub fn refresh_from_disk(&mut self) -> Result<bool, VaultError> {
        if self.state.is_none() || !self.changed_underneath() {
            return Ok(false);
        }
        self.absorb_remote()?;
        Ok(true)
    }

    /// Merge what is on disk into what is in memory.
    fn absorb_remote(&mut self) -> Result<(), VaultError> {
        let state = self.state.as_mut().ok_or(VaultError::Locked)?;
        let plaintext = load_document(&self.path, &state.password)?;
        let remote: VaultDocument =
            serde_json::from_slice(&plaintext).map_err(|_| VaultError::BadFormat)?;

        state.document = merge(state.document.clone(), remote);
        self.last_seen = self.stamp();
        Ok(())
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
        self.last_seen = self.stamp();
        let mut document: VaultDocument =
            serde_json::from_slice(&plaintext).map_err(|_| VaultError::BadFormat)?;
        document.purge_expired_tombstones(chrono::Utc::now());
        document.migrate_groups_to_folders();

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
        // Read before write. Another machine sharing this file may have added
        // an account since this one last looked, and writing over it would lose
        // that account with no error and no way to notice.
        if self.state.is_some() && self.changed_underneath() {
            self.absorb_remote()?;
        }

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

    fn write(&mut self, password: &str, document: &VaultDocument) -> Result<(), VaultError> {
        let plaintext = serde_json::to_vec(document).map_err(|e| VaultError::Io(e.to_string()))?;
        save_document_to(
            &self.path,
            &self.temp_path(),
            password,
            KdfParams::default(),
            &plaintext,
        )?;
        // Remember what we just wrote, so our own save does not read as someone
        // else's change on the next check.
        self.last_seen = self.stamp();
        Ok(())
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

    fn sample_named(issuer: &str) -> Account {
        Account::new(
            issuer.into(),
            "you@example.com".into(),
            Secret::from_bytes(b"12345678901234567890".to_vec()),
        )
    }

    #[test]
    fn two_managers_on_one_file_do_not_clobber_each_other() {
        // The whole point of the plan. Without the re-read, whichever saved
        // last would win and the other account would be gone.
        let mut a = manager("shared-a");
        a.create("master").unwrap();
        let path = a.path().to_path_buf();

        let mut b = VaultManager::new(path.clone());
        b.unlock("master").unwrap();

        a.mutate(|doc| doc.upsert(sample_named("Added on A")))
            .unwrap();
        b.mutate(|doc| doc.upsert(sample_named("Added on B")))
            .unwrap();

        let mut check = VaultManager::new(path);
        check.unlock("master").unwrap();
        let names: Vec<_> = check
            .document()
            .unwrap()
            .live()
            .map(|x| x.issuer.clone())
            .collect();

        assert!(
            names.contains(&"Added on A".to_string()),
            "A's account was lost: {names:?}"
        );
        assert!(
            names.contains(&"Added on B".to_string()),
            "B's account was lost: {names:?}"
        );
        cleanup(&check);
    }

    #[test]
    fn a_change_made_elsewhere_shows_up_on_refresh() {
        let mut a = manager("refresh");
        a.create("master").unwrap();
        let path = a.path().to_path_buf();

        let mut b = VaultManager::new(path);
        b.unlock("master").unwrap();
        b.mutate(|doc| doc.upsert(sample_named("Added on B")))
            .unwrap();

        assert!(a.refresh_from_disk().unwrap(), "the change went unnoticed");
        assert_eq!(a.document().unwrap().live().count(), 1);
        cleanup(&a);
    }

    #[test]
    fn refreshing_with_nothing_new_reports_nothing() {
        let mut a = manager("no-change");
        a.create("master").unwrap();
        assert!(!a.refresh_from_disk().unwrap());
        cleanup(&a);
    }

    #[test]
    fn refreshing_a_locked_vault_is_harmless() {
        let mut a = manager("refresh-locked");
        assert!(!a.refresh_from_disk().unwrap());
    }

    #[test]
    fn two_managers_never_share_a_temporary_file() {
        // One shared temporary name means two writers interleave into a
        // corrupt rename.
        let a = manager("tmpname");
        let b = VaultManager::new(a.path().to_path_buf());
        assert_ne!(a.temp_path(), b.temp_path(), "both would use one file");
    }
}
