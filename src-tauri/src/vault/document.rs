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
        assert_ne!(
            VaultDocument::new().device_id,
            VaultDocument::new().device_id
        );
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
