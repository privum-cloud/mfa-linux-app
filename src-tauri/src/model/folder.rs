//! A folder: somewhere to put accounts so forty of them stay findable.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A named place, optionally inside another one.
///
/// The synchronisation fields are here from the first version on purpose.
/// `revision` decides a merge — not `updated_at`, because a machine with a
/// skewed clock would otherwise win every one. `deleted_at` is a tombstone,
/// without which a deletion cannot reach the user's other machines. Adding
/// either later would mean migrating vaults that already exist.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: Uuid,
    pub name: String,
    pub icon: Option<String>,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub revision: u64,
}

impl Folder {
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            icon: None,
            parent_id: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            revision: 1,
        }
    }

    /// Record a local edit. Every mutation must go through this.
    pub fn touch(&mut self) {
        self.revision += 1;
        self.updated_at = Utc::now();
    }

    /// Mark the folder deleted without discarding it, so the deletion can reach
    /// the user's other machines.
    pub fn soft_delete(&mut self) {
        self.deleted_at = Some(Utc::now());
        self.touch();
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_folder_is_at_the_top_level_with_no_icon() {
        let folder = Folder::new("Example Client".into());
        assert_eq!(folder.name, "Example Client");
        assert_eq!(folder.parent_id, None);
        assert_eq!(folder.icon, None);
        assert_eq!(folder.revision, 1);
        assert!(!folder.is_deleted());
    }

    #[test]
    fn touch_advances_the_revision() {
        let mut folder = Folder::new("Example Client".into());
        folder.touch();
        assert_eq!(folder.revision, 2);
    }

    #[test]
    fn soft_delete_leaves_a_tombstone() {
        let mut folder = Folder::new("Example Client".into());
        folder.soft_delete();
        assert!(folder.is_deleted());
        assert!(folder.deleted_at.is_some());
        assert!(folder.revision > 1, "a delete must advance the revision");
    }

    #[test]
    fn two_folders_never_share_an_identifier() {
        assert_ne!(
            Folder::new("A".into()).id,
            Folder::new("A".into()).id,
            "same name must not mean same folder"
        );
    }
}
