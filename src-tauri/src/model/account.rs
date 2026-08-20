//! An account: the thing a code is generated for.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::otp::{Algorithm, Secret};

/// What kind of one-time password this account issues.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    #[default]
    Totp,
    Hotp,
    Steam,
}

/// An account Tessera generates codes for.
///
/// Two fields exist for synchronisation rather than for display:
///
/// `revision` is incremented on every local edit and is the authority when two
/// machines disagree. A wall clock is not: a laptop with a skewed clock would
/// otherwise win every merge, permanently and invisibly.
///
/// `deleted_at` is a tombstone. Removing the record outright would make the
/// deletion unpropagatable — the next merge would see the account alive on the
/// other machine and bring it back.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub issuer: String,
    pub label: String,
    pub secret: Secret,
    pub kind: AccountKind,
    pub algorithm: Algorithm,
    pub digits: u32,
    pub period: u32,
    /// HOTP only. The one mutable field, which is why merging takes its maximum
    /// rather than the most recent value — lost increments lock the user out.
    pub counter: u64,
    pub icon: Option<String>,
    pub group: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub revision: u64,
}

impl Account {
    /// A new TOTP account with the defaults nearly every service issues:
    /// HMAC-SHA1, six digits, thirty seconds.
    pub fn new(issuer: String, label: String, secret: Secret) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            issuer,
            label,
            secret,
            kind: AccountKind::Totp,
            algorithm: Algorithm::Sha1,
            digits: 6,
            period: 30,
            counter: 0,
            icon: None,
            group: None,
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

    /// Mark the account deleted without discarding it, so the deletion can
    /// reach the user's other machines.
    pub fn soft_delete(&mut self) {
        self.deleted_at = Some(Utc::now());
        self.touch();
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// How the account reads in the list.
    pub fn display_name(&self) -> String {
        if self.issuer.is_empty() {
            self.label.clone()
        } else {
            format!("{} ({})", self.issuer, self.label)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Account {
        Account::new(
            "GitHub".into(),
            "marcio@privum.cloud".into(),
            Secret::from_bytes(b"12345678901234567890".to_vec()),
        )
    }

    #[test]
    fn a_new_account_defaults_to_the_shape_services_actually_issue() {
        let account = sample();
        assert_eq!(account.kind, AccountKind::Totp);
        assert_eq!(account.algorithm, Algorithm::Sha1);
        assert_eq!(account.digits, 6);
        assert_eq!(account.period, 30);
        assert_eq!(account.counter, 0);
        assert_eq!(account.revision, 1);
        assert!(!account.is_deleted());
    }

    #[test]
    fn touch_advances_the_revision() {
        // revision, not updated_at, is what decides a merge — a machine with a
        // skewed clock must not be able to win one.
        let mut account = sample();
        let before = account.revision;
        account.touch();
        assert_eq!(account.revision, before + 1);
    }

    #[test]
    fn soft_delete_leaves_a_tombstone_rather_than_removing_the_record() {
        // Without a tombstone a delete cannot propagate: the next sync would see
        // the account present on the other machine and resurrect it.
        let mut account = sample();
        let before = account.revision;
        account.soft_delete();
        assert!(account.is_deleted());
        assert!(account.deleted_at.is_some());
        assert!(
            account.revision > before,
            "a delete must advance the revision"
        );
    }

    #[test]
    fn display_name_joins_issuer_and_label_but_copes_without_an_issuer() {
        assert_eq!(sample().display_name(), "GitHub (marcio@privum.cloud)");

        let mut anonymous = sample();
        anonymous.issuer = String::new();
        assert_eq!(anonymous.display_name(), "marcio@privum.cloud");
    }

    #[test]
    fn survives_a_serde_round_trip_with_every_field_intact() {
        // Every field of the Google Authenticator protobuf and the otpauth URI
        // is carried, even where the interface does not yet expose it. Dropping
        // one on import would lose data the user cannot recover.
        let mut original = sample();
        original.kind = AccountKind::Hotp;
        original.algorithm = Algorithm::Sha512;
        original.digits = 8;
        original.period = 60;
        original.counter = 42;
        original.icon = Some("github".into());
        original.group = Some("Work".into());

        let json = serde_json::to_string(&original).unwrap();
        let restored: Account = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.id, original.id);
        assert_eq!(restored.kind, original.kind);
        assert_eq!(restored.algorithm, original.algorithm);
        assert_eq!(restored.digits, original.digits);
        assert_eq!(restored.period, original.period);
        assert_eq!(restored.counter, original.counter);
        assert_eq!(restored.icon, original.icon);
        assert_eq!(restored.group, original.group);
        assert_eq!(restored.secret, original.secret);
        assert_eq!(restored.revision, original.revision);
    }
}
