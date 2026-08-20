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
    let mut rows: Vec<_> = document.live().map(|a| view_of(a, unix_seconds)).collect();
    rows.sort_by_key(|r| (r.issuer.to_lowercase(), r.label.to_lowercase()));
    rows
}

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
pub fn add_account_from_uri(state: tauri::State<'_, AppState>, uri: String) -> Result<(), String> {
    let account = parse_otpauth(&uri).map_err(|e| e.to_string())?;
    vault(&state)?
        .mutate(|doc| doc.upsert(account))
        .map_err(fail)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
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
    let mut guard = vault(&state)?;
    let mut account = guard
        .document()
        .map_err(fail)?
        .find(id)
        .ok_or_else(|| "that account is no longer in the vault".to_owned())?
        .clone();

    account.issuer = issuer;
    account.label = label;
    account.group = group;
    account.touch();

    guard.mutate(|doc| doc.upsert(account)).map_err(fail)
}

#[tauri::command]
pub fn delete_account(state: tauri::State<'_, AppState>, id: Uuid) -> Result<(), String> {
    let mut guard = vault(&state)?;
    let mut account = guard
        .document()
        .map_err(fail)?
        .find(id)
        .ok_or_else(|| "that account is no longer in the vault".to_owned())?
        .clone();

    // A tombstone, not a removal: without one the deletion cannot reach the
    // user's other machines and the next sync brings the account back.
    account.soft_delete();
    guard.mutate(|doc| doc.upsert(account)).map_err(fail)
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
        assert!(
            !json.contains("secret"),
            "AccountView exposed a secret: {json}"
        );
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
        let names: Vec<_> = rows
            .iter()
            .map(|r| format!("{}:{}", r.issuer, r.label))
            .collect();
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
