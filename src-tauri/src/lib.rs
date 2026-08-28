mod commands;
pub mod import;
pub mod model;
pub mod otp;
pub mod sync;
pub mod update;
pub mod vault;

use std::sync::Mutex;

use tauri::Manager;

use commands::AppState;
use vault::{Location, VaultManager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init());

    // Desktop only: a phone has no package to replace.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .setup(|app| {
            app.manage(AppState {
                vault: Mutex::new(VaultManager::new(Location::load().vault_path())),
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
            commands::vault_location,
            commands::set_vault_location,
            commands::refresh_vault,
            commands::update_policy,
            commands::set_update_check,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tessera");
}
