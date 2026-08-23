//! The Tauri command surface — the only part of the core the interface sees.
//!
//! Commands return codes and metadata. A raw secret never travels in this
//! direction: `AccountView` has no field for one, which is the point.

use std::sync::Mutex;

use serde::Serialize;
use uuid::Uuid;

use crate::import::parse_otpauth;
use crate::model::{Account, AccountKind, Folder};
use crate::otp::{seconds_remaining, steam_at, totp_at, Algorithm, Secret};
use crate::vault::{Settings, VaultDocument, VaultError, VaultManager};

/// Everything the interface needs to draw one row, and nothing more.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AccountView {
    pub id: Uuid,
    pub issuer: String,
    pub label: String,
    pub group: Option<String>,
    pub folder_id: Option<Uuid>,
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
        folder_id: account.folder_id,
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
    let mut guard = vault(&state)?;
    let timeout = guard.idle_timeout();
    Ok(guard.lock_if_idle(timeout))
}

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    let guard = vault(&state)?;
    Ok(guard.document().map_err(fail)?.settings.validated())
}

#[tauri::command]
pub fn set_settings(
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    let clean = settings.validated();
    vault(&state)?
        .mutate(|doc| {
            doc.settings = clean;
            doc.settings_revision += 1;
        })
        .map_err(fail)?;
    Ok(clean)
}

/// Note that the user did something. The interface calls this on interaction so
/// the idle timer measures inactivity rather than elapsed time.
#[tauri::command]
pub fn note_activity(state: tauri::State<'_, AppState>) -> Result<(), String> {
    vault(&state)?.touch_activity();
    Ok(())
}

/// What an import did, so the interface can say so plainly.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct ImportSummary {
    pub added: u32,
    pub already_present: u32,
}

/// Add imported accounts, skipping ones the vault already holds.
///
/// Matching is on the secret and not the identifier: an account read from a
/// phone carries a fresh id every time it is exported, so matching by id would
/// duplicate everything on every retry — and retrying is the normal case,
/// because people repeat an import when they are not sure it worked.
///
/// Tombstones are deliberately not matched against: a deleted account is one
/// the user removed, and importing the same secret again is them asking for it
/// back.
fn merge_imported(document: &mut VaultDocument, incoming: Vec<Account>) -> ImportSummary {
    let mut summary = ImportSummary {
        added: 0,
        already_present: 0,
    };

    for account in incoming {
        let known = document
            .accounts
            .iter()
            .any(|existing| !existing.is_deleted() && existing.secret == account.secret);

        if known {
            summary.already_present += 1;
        } else {
            document.upsert(account);
            summary.added += 1;
        }
    }
    summary
}

#[tauri::command]
pub fn import_from_migration_uri(
    state: tauri::State<'_, AppState>,
    uri: String,
) -> Result<ImportSummary, String> {
    let accounts = crate::import::parse_migration(&uri).map_err(|e| e.to_string())?;
    let mut summary = ImportSummary {
        added: 0,
        already_present: 0,
    };
    vault(&state)?
        .mutate(|doc| summary = merge_imported(doc, accounts))
        .map_err(fail)?;
    Ok(summary)
}

/// Import every account from every QR code in an image file.
#[tauri::command]
pub fn import_from_image(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<ImportSummary, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("could not read that file: {e}"))?;
    let payloads = crate::import::read_qr_codes(&bytes).map_err(|e| e.to_string())?;

    // One image may hold several codes, and a Google export of many accounts is
    // several codes. Take everything that parses; refuse only if nothing did.
    let mut accounts = Vec::new();
    let mut unreadable = 0;
    for payload in &payloads {
        if let Ok(mut batch) = crate::import::parse_migration(payload) {
            accounts.append(&mut batch);
        } else if let Ok(single) = crate::import::parse_otpauth(payload) {
            accounts.push(single);
        } else {
            unreadable += 1;
        }
    }

    if accounts.is_empty() {
        return Err(if unreadable > 0 {
            "that image holds QR codes, but none of them are accounts".to_owned()
        } else {
            "there is no QR code in that image".to_owned()
        });
    }

    let mut summary = ImportSummary {
        added: 0,
        already_present: 0,
    };
    vault(&state)?
        .mutate(|doc| summary = merge_imported(doc, accounts))
        .map_err(fail)?;
    Ok(summary)
}

/// Render the whole vault as Google Authenticator QR codes, as PNG data URLs.
///
/// The payload carries every secret in the vault, so it becomes a picture here
/// in Rust and never crosses into the interface as text.
#[tauri::command]
pub fn export_migration_qrs(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    use base64::Engine;

    let guard = vault(&state)?;
    let accounts: Vec<_> = guard.document().map_err(fail)?.live().cloned().collect();
    if accounts.is_empty() {
        return Err("there is nothing to export yet".to_owned());
    }

    crate::import::to_migration_uris(&accounts, crate::import::ACCOUNTS_PER_BATCH)
        .iter()
        .map(|uri| {
            let png = crate::import::render_qr_png(uri).map_err(|e| e.to_string())?;
            Ok(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&png)
            ))
        })
        .collect()
}

/// One row of the folder tree, flattened for rendering.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct FolderView {
    pub id: Uuid,
    pub name: String,
    pub icon: Option<String>,
    pub parent_id: Option<Uuid>,
    /// How deep to indent this row.
    pub depth: u32,
    pub account_count: u32,
}

/// Flatten the folder tree depth-first, siblings in name order.
///
/// Ordering is case-insensitive and stable for the same reason the account list
/// is: a tree whose rows move between openings is one you misclick.
fn folder_views(document: &VaultDocument) -> Vec<FolderView> {
    fn walk(document: &VaultDocument, parent: Option<Uuid>, depth: u32, out: &mut Vec<FolderView>) {
        let mut children: Vec<_> = document
            .live_folders()
            .filter(|f| f.parent_id == parent)
            .collect();
        children.sort_by_key(|f| f.name.to_lowercase());

        for folder in children {
            out.push(FolderView {
                id: folder.id,
                name: folder.name.clone(),
                icon: folder.icon.clone(),
                parent_id: folder.parent_id,
                depth,
                account_count: document
                    .live()
                    .filter(|a| a.folder_id == Some(folder.id))
                    .count() as u32,
            });
            walk(document, Some(folder.id), depth + 1, out);
        }
    }

    let mut out = Vec::new();
    walk(document, None, 0, &mut out);
    out
}

#[tauri::command]
pub fn list_folders(state: tauri::State<'_, AppState>) -> Result<Vec<FolderView>, String> {
    let guard = vault(&state)?;
    Ok(folder_views(guard.document().map_err(fail)?))
}

#[tauri::command]
pub fn create_folder(
    state: tauri::State<'_, AppState>,
    name: String,
    parent_id: Option<Uuid>,
) -> Result<(), String> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err("give the folder a name".to_owned());
    }
    let mut folder = Folder::new(name);
    folder.parent_id = parent_id;

    vault(&state)?
        .mutate(|doc| doc.upsert_folder(folder))
        .map_err(fail)
}

/// Fetch a folder to edit, failing with a message rather than silently.
fn folder_for_edit(guard: &VaultManager, id: Uuid) -> Result<Folder, String> {
    guard
        .document()
        .map_err(fail)?
        .find_folder(id)
        .ok_or_else(|| "that folder is no longer in the vault".to_owned())
        .cloned()
}

#[tauri::command]
pub fn rename_folder(
    state: tauri::State<'_, AppState>,
    id: Uuid,
    name: String,
) -> Result<(), String> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err("give the folder a name".to_owned());
    }
    let mut guard = vault(&state)?;
    let mut folder = folder_for_edit(&guard, id)?;
    folder.name = name;
    folder.touch();
    guard.mutate(|doc| doc.upsert_folder(folder)).map_err(fail)
}

#[tauri::command]
pub fn set_folder_icon(
    state: tauri::State<'_, AppState>,
    id: Uuid,
    icon: Option<String>,
) -> Result<(), String> {
    let mut guard = vault(&state)?;
    let mut folder = folder_for_edit(&guard, id)?;
    folder.icon = icon.filter(|i| !i.is_empty());
    folder.touch();
    guard.mutate(|doc| doc.upsert_folder(folder)).map_err(fail)
}

#[tauri::command]
pub fn move_folder(
    state: tauri::State<'_, AppState>,
    id: Uuid,
    parent_id: Option<Uuid>,
) -> Result<(), String> {
    let mut guard = vault(&state)?;
    if guard.document().map_err(fail)?.would_cycle(id, parent_id) {
        return Err("a folder cannot go inside itself".to_owned());
    }
    let mut folder = folder_for_edit(&guard, id)?;
    folder.parent_id = parent_id;
    folder.touch();
    guard.mutate(|doc| doc.upsert_folder(folder)).map_err(fail)
}

/// Delete a folder. Its accounts and subfolders move up; nothing is lost.
#[tauri::command]
pub fn remove_folder(state: tauri::State<'_, AppState>, id: Uuid) -> Result<(), String> {
    vault(&state)?
        .mutate(|doc| doc.delete_folder(id))
        .map_err(fail)
}

#[tauri::command]
pub fn move_account_to_folder(
    state: tauri::State<'_, AppState>,
    id: Uuid,
    folder_id: Option<Uuid>,
) -> Result<(), String> {
    let mut guard = vault(&state)?;
    let mut account = guard
        .document()
        .map_err(fail)?
        .find(id)
        .ok_or_else(|| "that account is no longer in the vault".to_owned())?
        .clone();
    account.folder_id = folder_id;
    account.touch();
    guard.mutate(|doc| doc.upsert(account)).map_err(fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn totp_account() -> Account {
        parse_otpauth(&format!(
            "otpauth://totp/GitHub:you@example.com?secret={SECRET_B32}&digits=8"
        ))
        .unwrap()
    }

    #[test]
    fn renders_a_row_whose_code_matches_the_rfc_vector() {
        let view = view_of(&totp_account(), 59);
        assert_eq!(view.code, "94287082");
        assert_eq!(view.seconds_remaining, 1);
        assert_eq!(view.issuer, "GitHub");
        assert_eq!(view.label, "you@example.com");
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

    #[test]
    fn an_import_reports_what_it_added_and_what_it_already_had() {
        // Importing the same export twice is the normal case — a person retries
        // when they are unsure it worked. Silently duplicating every account
        // would be the worst possible answer.
        let mut doc = VaultDocument::new();
        doc.upsert(totp_account());

        let incoming = vec![totp_account(), sample_other()];
        let summary = merge_imported(&mut doc, incoming);

        assert_eq!(summary.added, 1);
        assert_eq!(summary.already_present, 1);
        assert_eq!(doc.live().count(), 2, "an account was duplicated");
    }

    #[test]
    fn importing_matches_on_the_secret_not_the_identifier() {
        // An account read from a phone carries a fresh id every time it is
        // exported, so matching by id would duplicate on every retry.
        let mut doc = VaultDocument::new();
        doc.upsert(totp_account());

        let mut renamed = totp_account();
        renamed.issuer = "GitHub (renamed on the phone)".into();

        let summary = merge_imported(&mut doc, vec![renamed]);
        assert_eq!(summary.added, 0);
        assert_eq!(summary.already_present, 1);
    }

    #[test]
    fn a_deleted_account_can_be_imported_again() {
        // A tombstone means the user removed it. Bringing the same secret back
        // deliberately must work, or a delete becomes permanent by accident.
        let mut doc = VaultDocument::new();
        let mut gone = totp_account();
        gone.soft_delete();
        doc.upsert(gone);

        let summary = merge_imported(&mut doc, vec![totp_account()]);
        assert_eq!(summary.added, 1);
        assert_eq!(doc.live().count(), 1);
    }

    fn sample_other() -> Account {
        parse_otpauth("otpauth://totp/Google:alice?secret=JBSWY3DPEHPK3PXP").unwrap()
    }

    #[test]
    fn folders_are_listed_depth_first_so_the_tree_reads_top_to_bottom() {
        let mut doc = VaultDocument::new();
        let clients = Folder::new("Clients".into());
        let mut one = Folder::new("Example Client".into());
        one.parent_id = Some(clients.id);
        let personal = Folder::new("Personal".into());
        for f in [clients.clone(), one.clone(), personal.clone()] {
            doc.upsert_folder(f);
        }

        let rows = folder_views(&doc);
        let shape: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.depth)).collect();
        assert_eq!(
            shape,
            vec![("Clients", 0), ("Example Client", 1), ("Personal", 0)]
        );
    }

    #[test]
    fn a_folder_row_counts_the_accounts_inside_it() {
        let mut doc = VaultDocument::new();
        let client = Folder::new("Example Client".into());
        let mut acc = totp_account();
        acc.folder_id = Some(client.id);
        let mut gone = sample_other();
        gone.folder_id = Some(client.id);
        gone.soft_delete();
        doc.upsert_folder(client.clone());
        doc.upsert(acc);
        doc.upsert(gone);

        let rows = folder_views(&doc);
        assert_eq!(rows[0].account_count, 1, "a tombstone was counted");
    }

    #[test]
    fn sibling_folders_are_ordered_by_name_so_the_tree_does_not_reshuffle() {
        let mut doc = VaultDocument::new();
        for name in ["zulu", "Alpha", "middle"] {
            doc.upsert_folder(Folder::new(name.into()));
        }
        let names: Vec<_> = folder_views(&doc).iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, vec!["Alpha", "middle", "zulu"]);
    }
}
