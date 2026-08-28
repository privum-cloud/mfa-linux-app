//! Where the vault file lives.
//!
//! This is the one preference that cannot be stored inside the vault: the path
//! is needed before there is anything to unlock. It sits in a small file with
//! no secrets in it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::vault::{default_vault_path, VaultError};

/// On unless the user turned it off. Someone who never opens settings should
/// still hear about a release that fixes something.
fn enabled() -> bool {
    true
}

/// Preferences that must be readable before the vault is open.
#[derive(Debug, Serialize, Deserialize)]
pub struct Location {
    /// Empty or absent means the default place.
    #[serde(default)]
    vault_path: String,
    /// Identifies this machine, generated once and kept out of the vault.
    ///
    /// The `device_id` inside the vault document cannot do this job: it travels
    /// with the file, so every machine sharing a vault reports the same one.
    #[serde(default)]
    device_id: String,
    /// Whether to ask GitHub about newer releases.
    ///
    /// This cannot live with the other settings inside the sealed document: the
    /// check runs while the vault is still locked, which is exactly when
    /// someone who has not opened Tessera in a month most needs to hear that a
    /// new version exists.
    #[serde(default = "enabled")]
    check_for_updates: bool,
}

impl Default for Location {
    fn default() -> Self {
        Self {
            vault_path: String::new(),
            device_id: String::new(),
            check_for_updates: enabled(),
        }
    }
}

/// A short random identity for this machine.
fn new_device_id() -> String {
    let mut bytes = [0u8; 8];
    // A failure here would be a broken system RNG; falling back to a constant
    // only means two machines might share a temporary file name.
    let _ = getrandom::getrandom(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tessera")
        .join("config.json")
}

impl Location {
    pub fn load() -> Self {
        let mut location =
            Self::from_json(&std::fs::read_to_string(config_path()).unwrap_or_default());
        if location.device_id.is_empty() {
            location.device_id = new_device_id();
            // Best effort: an unwritable config only costs a fresh id next time.
            let _ = location.save();
        }
        location
    }

    /// Whether to ask GitHub about newer releases.
    pub fn checks_for_updates(&self) -> bool {
        self.check_for_updates
    }

    pub fn set_checks_for_updates(&mut self, on: bool) {
        self.check_for_updates = on;
    }

    /// This machine's identity, used to keep two writers off one temporary file.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Parse, falling back to the default on anything unexpected.
    ///
    /// A broken preferences file must never stand between someone and their
    /// second factors, so this cannot fail.
    pub fn from_json(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }

    pub fn vault_path(&self) -> PathBuf {
        if self.vault_path.trim().is_empty() {
            default_vault_path()
        } else {
            PathBuf::from(self.vault_path.trim())
        }
    }

    pub fn set_vault_path(&mut self, path: PathBuf) {
        self.vault_path = path.to_string_lossy().into_owned();
    }

    /// Whether the vault is somewhere the user chose.
    pub fn is_custom(&self) -> bool {
        !self.vault_path.trim().is_empty()
    }

    pub fn save(&self) -> Result<(), VaultError> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VaultError::Io(e.to_string()))?;
        }
        let body = serde_json::to_string_pretty(self).map_err(|e| VaultError::Io(e.to_string()))?;
        std::fs::write(&path, body).map_err(|e| VaultError::Io(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_no_config_the_vault_sits_in_the_default_place() {
        assert!(Location::from_json("")
            .vault_path()
            .ends_with("tessera/vault.bin"));
    }

    #[test]
    fn a_saved_path_is_read_back() {
        let stored = r#"{"vault_path":"/tmp/example/vault.bin"}"#;
        assert_eq!(
            Location::from_json(stored).vault_path(),
            std::path::Path::new("/tmp/example/vault.bin")
        );
    }

    #[test]
    fn a_corrupt_config_falls_back_rather_than_refusing_to_start() {
        // A broken preferences file must not stand between someone and their
        // second factors. The default is always openable.
        assert!(Location::from_json("{ this is not json")
            .vault_path()
            .ends_with("tessera/vault.bin"));
    }

    #[test]
    fn an_empty_or_blank_path_is_ignored() {
        for raw in [r#"{"vault_path":""}"#, r#"{"vault_path":"   "}"#] {
            assert!(
                Location::from_json(raw)
                    .vault_path()
                    .ends_with("tessera/vault.bin"),
                "failed on {raw}"
            );
        }
    }

    #[test]
    fn update_checking_is_on_until_someone_turns_it_off() {
        assert!(Location::from_json("").checks_for_updates());
    }

    #[test]
    fn a_config_written_before_this_setting_existed_still_gets_the_check() {
        // Every vault in the field was written by a version with no such key.
        // Defaulting to off would mean nobody already installed ever hears
        // about the release that fixes something for them.
        let older = r#"{"vault_path":"","device_id":"a1b2c3d4"}"#;
        assert!(Location::from_json(older).checks_for_updates());
    }

    #[test]
    fn turning_the_check_off_is_remembered() {
        assert!(!Location::from_json(r#"{"check_for_updates":false}"#).checks_for_updates());
    }

    #[test]
    fn the_preference_survives_being_written_and_read_back() {
        let mut location = Location::default();
        location.set_checks_for_updates(false);
        let written = serde_json::to_string(&location).unwrap();
        assert!(!Location::from_json(&written).checks_for_updates());
    }

    #[test]
    fn a_device_id_is_eight_bytes_of_hex() {
        let id = new_device_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn two_machines_do_not_share_an_identity() {
        assert_ne!(new_device_id(), new_device_id());
    }

    #[test]
    fn a_chosen_path_is_reported_as_custom() {
        assert!(!Location::from_json("").is_custom());
        assert!(Location::from_json(r#"{"vault_path":"/tmp/x.bin"}"#).is_custom());
    }
}
