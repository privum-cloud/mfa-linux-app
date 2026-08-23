mod commands;
pub mod import;
pub mod model;
pub mod otp;
pub mod sync;
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
            commands::get_settings,
            commands::set_settings,
            commands::import_from_image,
            commands::import_from_migration_uri,
            commands::export_migration_qrs,
            commands::list_folders,
            commands::create_folder,
            commands::rename_folder,
            commands::set_folder_icon,
            commands::move_folder,
            commands::remove_folder,
            commands::move_account_to_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tessera");
}
