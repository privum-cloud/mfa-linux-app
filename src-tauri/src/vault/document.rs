//! The document that gets sealed: every account, plus what synchronisation needs.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{Account, Folder};
use crate::vault::Settings;

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
    /// `serde(default)` so a vault written before folders existed still opens.
    #[serde(default)]
    pub folders: Vec<Folder>,
    /// `serde(default)` so a vault written before settings existed still opens.
    #[serde(default)]
    pub settings: Settings,
}

impl VaultDocument {
    pub fn new() -> Self {
        Self {
            version: CURRENT_VERSION,
            device_id: Uuid::new_v4(),
            accounts: Vec::new(),
            folders: Vec::new(),
            settings: Settings::default(),
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

    /// The folders a user should see.
    pub fn live_folders(&self) -> impl Iterator<Item = &Folder> {
        self.folders.iter().filter(|f| !f.is_deleted())
    }

    pub fn find_folder(&self, id: Uuid) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    pub fn upsert_folder(&mut self, folder: Folder) {
        match self.folders.iter_mut().find(|f| f.id == folder.id) {
            Some(existing) => *existing = folder,
            None => self.folders.push(folder),
        }
    }

    /// Delete a folder, moving everything it held up to its parent.
    ///
    /// Accounts are never deleted with it. Someone tidying their folders must
    /// not be able to lose a second factor by accident, and an account with no
    /// folder is merely untidy.
    pub fn delete_folder(&mut self, id: Uuid) {
        let grandparent = self.find_folder(id).and_then(|f| f.parent_id);

        for account in self.accounts.iter_mut() {
            if account.folder_id == Some(id) {
                account.folder_id = grandparent;
                account.touch();
            }
        }
        for folder in self.folders.iter_mut() {
            if folder.parent_id == Some(id) {
                folder.parent_id = grandparent;
                folder.touch();
            }
        }
        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == id) {
            folder.soft_delete();
        }
    }

    /// The chain from the root down to this folder.
    ///
    /// Bounded by the number of folders, so a cycle that reached the vault
    /// through a hand edit or a merge cannot hang the interface.
    pub fn folder_path(&self, id: Uuid) -> Vec<&Folder> {
        let mut chain = Vec::new();
        let mut at = Some(id);
        // A valid chain visits each folder at most once, so this is the exact
        // bound. One more would let a cycle produce a chain longer than the
        // tree, which is how the guard was wrong the first time.
        let mut guard = self.folders.len();

        while let Some(current) = at {
            if guard == 0 {
                break;
            }
            guard -= 1;
            match self.find_folder(current) {
                Some(folder) => {
                    chain.push(folder);
                    at = folder.parent_id;
                }
                None => break,
            }
        }
        chain.reverse();
        chain
    }

    /// Would moving `folder` under `new_parent` create a loop?
    pub fn would_cycle(&self, folder: Uuid, new_parent: Option<Uuid>) -> bool {
        let mut at = new_parent;
        let mut guard = self.folders.len();

        while let Some(current) = at {
            if current == folder {
                return true;
            }
            if guard == 0 {
                return true;
            }
            guard -= 1;
            at = self.find_folder(current).and_then(|f| f.parent_id);
        }
        false
    }

    /// Turn the retired free-text `group` field into real folders.
    ///
    /// Runs on unlock. Idempotent: a second pass finds the folders already
    /// there and the groups already cleared.
    pub fn migrate_groups_to_folders(&mut self) {
        let groups: Vec<String> = self
            .accounts
            .iter()
            .filter_map(|a| a.group.clone())
            .filter(|g| !g.trim().is_empty())
            .collect();

        for name in groups {
            let name = name.trim().to_owned();
            // Resolve first, so the immutable borrow ends before the push.
            let existing = self.live_folders().find(|f| f.name == name).map(|f| f.id);
            let id = match existing {
                Some(id) => id,
                None => {
                    let folder = Folder::new(name.clone());
                    let id = folder.id;
                    self.folders.push(folder);
                    id
                }
            };

            for account in self.accounts.iter_mut() {
                if account.group.as_deref().map(str::trim) == Some(name.as_str()) {
                    account.folder_id = Some(id);
                    account.group = None;
                    account.touch();
                }
            }
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
            "you@example.com".into(),
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

    #[test]
    fn a_document_written_before_settings_existed_still_opens() {
        // Vaults on disk predate this field. Losing them to a missing key would
        // be the worst possible bug in a program that holds second factors.
        let legacy =
            r#"{"version":1,"device_id":"00000000-0000-4000-8000-000000000000","accounts":[]}"#;
        let doc: VaultDocument = serde_json::from_str(legacy).unwrap();
        assert_eq!(doc.settings, crate::vault::Settings::default());
    }

    fn folder(name: &str) -> Folder {
        Folder::new(name.into())
    }

    #[test]
    fn deleting_a_folder_never_deletes_the_accounts_in_it() {
        // Losing a second factor because a folder was tidied away is the worst
        // outcome this feature could produce.
        let mut doc = VaultDocument::new();
        let client = folder("Example Client");
        let mut acc = account("GitHub");
        acc.folder_id = Some(client.id);
        doc.upsert_folder(client.clone());
        doc.upsert(acc);

        doc.delete_folder(client.id);

        assert_eq!(doc.live().count(), 1, "an account went with the folder");
        assert_eq!(
            doc.live().next().unwrap().folder_id,
            None,
            "it should be loose"
        );
        assert_eq!(doc.live_folders().count(), 0);
    }

    #[test]
    fn deleting_a_folder_moves_its_subfolders_up_rather_than_orphaning_them() {
        let mut doc = VaultDocument::new();
        let parent = folder("Clients");
        let mut child = folder("Example Client");
        child.parent_id = Some(parent.id);
        doc.upsert_folder(parent.clone());
        doc.upsert_folder(child.clone());

        doc.delete_folder(parent.id);

        let survivor = doc.find_folder(child.id).unwrap();
        assert!(!survivor.is_deleted(), "the child was deleted too");
        assert_eq!(
            survivor.parent_id, None,
            "the child was left pointing at a ghost"
        );
    }

    #[test]
    fn folder_path_reads_from_the_root_down() {
        let mut doc = VaultDocument::new();
        let root = folder("Clients");
        let mut mid = folder("Example Client");
        mid.parent_id = Some(root.id);
        let mut leaf = folder("Production");
        leaf.parent_id = Some(mid.id);
        for f in [root.clone(), mid.clone(), leaf.clone()] {
            doc.upsert_folder(f);
        }

        let names: Vec<_> = doc
            .folder_path(leaf.id)
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(names, vec!["Clients", "Example Client", "Production"]);
    }

    #[test]
    fn a_folder_cannot_be_moved_inside_itself_or_its_own_child() {
        // A cycle makes folder_path loop forever and the tree unrenderable.
        let mut doc = VaultDocument::new();
        let parent = folder("Clients");
        let mut child = folder("Example Client");
        child.parent_id = Some(parent.id);
        doc.upsert_folder(parent.clone());
        doc.upsert_folder(child.clone());

        assert!(doc.would_cycle(parent.id, Some(parent.id)), "into itself");
        assert!(
            doc.would_cycle(parent.id, Some(child.id)),
            "into its own child"
        );
        assert!(
            !doc.would_cycle(child.id, Some(parent.id)),
            "this one is fine"
        );
        assert!(
            !doc.would_cycle(child.id, None),
            "moving to the top is fine"
        );
    }

    #[test]
    fn folder_path_survives_a_cycle_that_reached_the_vault_somehow() {
        // would_cycle guards the door, but a hand-edited or merged vault could
        // still carry one. Looping forever inside a getter is not an option.
        let mut doc = VaultDocument::new();
        let mut a = folder("A");
        let mut b = folder("B");
        a.parent_id = Some(b.id);
        b.parent_id = Some(a.id);
        doc.upsert_folder(a.clone());
        doc.upsert_folder(b.clone());

        assert!(
            doc.folder_path(a.id).len() <= 2,
            "folder_path did not terminate"
        );
    }

    #[test]
    fn the_old_group_field_becomes_a_real_folder() {
        // `group` was free text with no interface behind it. Anyone who typed
        // one should find it as a folder rather than lose it.
        let mut doc = VaultDocument::new();
        let mut one = account("GitHub");
        one.group = Some("Example Client".into());
        let mut two = account("GitLab");
        two.group = Some("Example Client".into());
        doc.upsert(one);
        doc.upsert(two);

        doc.migrate_groups_to_folders();

        assert_eq!(
            doc.live_folders().count(),
            1,
            "one group should make one folder"
        );
        let created_id = doc.live_folders().next().unwrap().id;
        assert_eq!(doc.find_folder(created_id).unwrap().name, "Example Client");
        for a in doc.live() {
            assert_eq!(a.folder_id, Some(created_id));
            assert_eq!(a.group, None, "the old field should be cleared");
        }
    }

    #[test]
    fn migrating_twice_does_not_make_the_folder_twice() {
        let mut doc = VaultDocument::new();
        let mut one = account("GitHub");
        one.group = Some("Example Client".into());
        doc.upsert(one);

        doc.migrate_groups_to_folders();
        doc.migrate_groups_to_folders();

        assert_eq!(doc.live_folders().count(), 1);
    }
}
