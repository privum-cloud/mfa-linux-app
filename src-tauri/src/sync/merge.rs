//! Reconciling two copies of one vault.
//!
//! This is the only place the merge rules live, and it does no I/O: two
//! documents in, one document out. Everything here exists because the
//! alternative loses an account without saying so.

use crate::model::{Account, Folder};
use crate::vault::VaultDocument;

/// Reconcile two copies of one vault into one.
///
/// `local` is this machine's copy and wins nothing by being local except its
/// `device_id`, which identifies the machine rather than the document.
pub fn merge(mut local: VaultDocument, remote: VaultDocument) -> VaultDocument {
    for incoming in remote.accounts {
        match local.accounts.iter_mut().find(|a| a.id == incoming.id) {
            Some(existing) => *existing = pick_account(existing.clone(), incoming),
            None => local.accounts.push(incoming),
        }
    }

    for incoming in remote.folders {
        match local.folders.iter_mut().find(|f| f.id == incoming.id) {
            Some(existing) => *existing = pick_folder(existing.clone(), incoming),
            None => local.folders.push(incoming),
        }
    }

    // Settings carry their own counter, because they are one value rather than
    // a collection and there is no per-field revision to compare.
    if remote.settings_revision > local.settings_revision {
        local.settings = remote.settings;
        local.settings_revision = remote.settings_revision;
    }

    local
}

/// Which of two versions of the same account survives.
///
/// No special case for tombstones is needed: `soft_delete` calls `touch`, so a
/// deletion always carries a higher revision than the record it deleted, and
/// the ordinary comparison already prefers it. An edit made later still wins,
/// which is correct — someone who deletes on one machine and then edits on
/// another meant the edit.
fn pick_account(mine: Account, theirs: Account) -> Account {
    // The counter is settled before the winner is, and independently of it. It
    // only ever moves forward, and being ahead of the server is recoverable
    // through its resynchronisation window while being behind is not. Taking
    // the winner's counter would drop the loser's increments and lock the user
    // out of the account.
    let counter = mine.counter.max(theirs.counter);

    let mut winner = if wins(
        mine.revision,
        mine.updated_at,
        theirs.revision,
        theirs.updated_at,
    ) {
        mine
    } else {
        theirs
    };
    winner.counter = counter;
    winner
}

fn pick_folder(mine: Folder, theirs: Folder) -> Folder {
    if wins(
        mine.revision,
        mine.updated_at,
        theirs.revision,
        theirs.updated_at,
    ) {
        mine
    } else {
        theirs
    }
}

/// The comparison both types share.
///
/// Revision first: it is incremented locally on every edit and cannot be skewed
/// by a wrong clock. `updated_at` breaks a tie only, because two independent
/// machines can genuinely reach the same revision.
fn wins(
    mine_revision: u64,
    mine_updated: chrono::DateTime<chrono::Utc>,
    theirs_revision: u64,
    theirs_updated: chrono::DateTime<chrono::Utc>,
) -> bool {
    match mine_revision.cmp(&theirs_revision) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => mine_updated >= theirs_updated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AccountKind;
    use crate::otp::Secret;
    use chrono::{Duration, Utc};

    fn account(issuer: &str) -> Account {
        Account::new(
            issuer.into(),
            "you@example.com".into(),
            Secret::from_bytes(b"12345678901234567890".to_vec()),
        )
    }

    #[test]
    fn an_account_only_one_side_has_survives() {
        let mut local = VaultDocument::new();
        let mut remote = VaultDocument::new();
        local.upsert(account("GitHub"));
        remote.upsert(account("GitLab"));

        assert_eq!(
            merge(local, remote).live().count(),
            2,
            "an account was dropped"
        );
    }

    #[test]
    fn the_higher_revision_wins_whatever_the_clock_says() {
        // The whole reason revision exists: a machine with a skewed clock must
        // not be able to win every merge, permanently and invisibly.
        let mut local = VaultDocument::new();
        let mut newer = account("GitHub");
        newer.issuer = "GitHub Enterprise".into();
        newer.revision = 5;
        newer.updated_at = Utc::now() - Duration::days(365);
        local.upsert(newer);

        let mut remote = VaultDocument::new();
        let mut older = local.accounts[0].clone();
        older.issuer = "GitHub".into();
        older.revision = 2;
        older.updated_at = Utc::now();
        remote.upsert(older);

        assert_eq!(
            merge(local, remote).live().next().unwrap().issuer,
            "GitHub Enterprise"
        );
    }

    #[test]
    fn updated_at_only_breaks_a_tie_between_equal_revisions() {
        let mut local = VaultDocument::new();
        let mut a = account("GitHub");
        a.revision = 3;
        a.updated_at = Utc::now() - Duration::hours(1);
        local.upsert(a.clone());

        let mut remote = VaultDocument::new();
        let mut b = a.clone();
        b.issuer = "Newer".into();
        b.updated_at = Utc::now();
        remote.upsert(b);

        assert_eq!(merge(local, remote).live().next().unwrap().issuer, "Newer");
    }

    #[test]
    fn a_deletion_reaches_the_other_machine() {
        let mut local = VaultDocument::new();
        let live = account("GitHub");
        local.upsert(live.clone());

        let mut remote = VaultDocument::new();
        let mut gone = live.clone();
        gone.soft_delete();
        remote.upsert(gone);

        let merged = merge(local, remote);
        assert_eq!(merged.live().count(), 0, "the deletion did not travel");
        assert_eq!(merged.accounts.len(), 1, "the tombstone was thrown away");
    }

    #[test]
    fn an_edit_newer_than_a_deletion_brings_the_account_back() {
        // A tombstone is not permanent: someone who deletes on one machine and
        // then edits on another meant the edit.
        let mut remote = VaultDocument::new();
        let mut gone = account("GitHub");
        gone.soft_delete();
        remote.upsert(gone.clone());

        let mut local = VaultDocument::new();
        let mut revived = gone.clone();
        revived.deleted_at = None;
        revived.revision = 9;
        local.upsert(revived);

        assert_eq!(
            merge(local, remote).live().count(),
            1,
            "the later edit lost to a tombstone"
        );
    }

    #[test]
    fn an_hotp_counter_takes_the_maximum_rather_than_the_newest() {
        // The rule that matters most. A counter advances every time a code is
        // generated; overwriting it with the newer value drops the other
        // machine's increments, and a token behind the server locks the user
        // out of that account.
        let mut local = VaultDocument::new();
        let mut here = account("Example Bank");
        here.kind = AccountKind::Hotp;
        here.counter = 40;
        here.revision = 9;
        local.upsert(here.clone());

        let mut remote = VaultDocument::new();
        let mut there = here.clone();
        there.counter = 57;
        there.revision = 2;
        remote.upsert(there);

        assert_eq!(
            merge(local, remote).live().next().unwrap().counter,
            57,
            "increments were lost"
        );
    }

    #[test]
    fn folders_merge_by_the_same_rules_as_accounts() {
        let mut local = VaultDocument::new();
        let mut here = Folder::new("Clients".into());
        here.revision = 5;
        local.upsert_folder(here.clone());

        let mut remote = VaultDocument::new();
        let mut there = here.clone();
        there.name = "Old name".into();
        there.revision = 2;
        remote.upsert_folder(there);
        remote.upsert_folder(Folder::new("Personal".into()));

        let merged = merge(local, remote);
        assert_eq!(merged.live_folders().count(), 2);
        assert!(merged.live_folders().any(|f| f.name == "Clients"));
    }

    #[test]
    fn settings_follow_their_own_revision() {
        let mut local = VaultDocument::new();
        local.settings.idle_timeout_secs = 60;
        local.settings_revision = 1;

        let mut remote = VaultDocument::new();
        remote.settings.idle_timeout_secs = 900;
        remote.settings_revision = 4;

        assert_eq!(merge(local, remote).settings.idle_timeout_secs, 900);
    }

    #[test]
    fn the_device_id_of_the_local_side_is_kept() {
        // device_id identifies this machine, not this document. Taking the
        // remote one would make both machines claim the same identity.
        let local = VaultDocument::new();
        let remote = VaultDocument::new();
        let mine = local.device_id;

        assert_eq!(merge(local, remote).device_id, mine);
    }

    #[test]
    fn merging_is_commutative() {
        // Whichever machine syncs first, both must land on the same vault.
        let (a, b) = two_diverged_vaults();

        let one = merge(a.clone(), b.clone());
        let other = merge(b, a);

        let names = |d: &VaultDocument| {
            let mut v: Vec<_> = d.live().map(|x| x.issuer.clone()).collect();
            v.sort();
            v
        };
        assert_eq!(
            names(&one),
            names(&other),
            "the order of the merge mattered"
        );
    }

    #[test]
    fn merging_twice_changes_nothing_the_second_time() {
        let (a, b) = two_diverged_vaults();

        let once = merge(a.clone(), b.clone());
        let twice = merge(once.clone(), b);

        assert_eq!(once.accounts.len(), twice.accounts.len());
        assert_eq!(once.live().count(), twice.live().count());
    }

    /// Two copies that drifted apart the way two machines actually do: one
    /// added on each side, one edited on one, one HOTP used on the other.
    fn two_diverged_vaults() -> (VaultDocument, VaultDocument) {
        let shared = account("GitHub");
        let mut counter_account = account("Example Bank");
        counter_account.kind = AccountKind::Hotp;

        let mut a = VaultDocument::new();
        let mut b = VaultDocument::new();

        for doc in [&mut a, &mut b] {
            doc.upsert(shared.clone());
            doc.upsert(counter_account.clone());
        }

        a.upsert(account("Only on A"));
        b.upsert(account("Only on B"));

        let mut edited = shared.clone();
        edited.issuer = "GitHub, renamed on A".into();
        edited.revision = 7;
        a.upsert(edited);

        let mut used = counter_account.clone();
        used.counter = 12;
        used.revision = 3;
        b.upsert(used);

        (a, b)
    }
}
