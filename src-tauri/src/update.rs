//! How a new version reaches an installation.
//!
//! Every desktop package Tessera ships can be replaced in place, but not on the
//! same terms. An AppImage is a single file the running process owns, the
//! Windows installer runs again over the top, and a macOS `.app` is a folder
//! in the user's own `/Applications`; all three finish without asking anyone
//! for anything. A `.deb` or `.rpm` belongs to the system package manager, so
//! replacing one runs `dpkg`/`rpm` through `pkexec` and the system puts up its
//! administrator prompt.
//!
//! That difference is not an implementation detail to hide. Someone who clicks
//! "Update" and is unexpectedly asked for a root password learns to type root
//! passwords into whatever asks — which is the opposite of what an
//! authenticator should be teaching. So the interface says which of the two is
//! about to happen, before the click.

use serde::Serialize;

/// Where someone is sent when Tessera cannot finish the job itself.
pub const RELEASES_URL: &str = "https://github.com/privum-cloud/tessera-mfa-app/releases/latest";

/// What installing a new version will cost the person watching.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// Replaces itself and restarts. Nothing is asked of the user.
    SelfInstall,
    /// Can replace itself, but the system will ask for an administrator
    /// password first, because the package manager owns the files.
    NeedsAdmin,
}

/// Decide from what the running process can see.
///
/// `self_replacing` is true on Windows and macOS, where the installed copy
/// belongs to the user rather than to a package manager.
///
/// `appimage` is the `APPIMAGE` environment variable, which the AppImage
/// runtime sets to the path of the image it is running. Nothing else sets it,
/// so its presence is what separates an AppImage from a `.deb` or `.rpm`
/// unpacked into the same place on disk.
pub fn delivery_from(self_replacing: bool, appimage: Option<&str>) -> Delivery {
    if self_replacing || appimage.is_some_and(|path| !path.is_empty()) {
        Delivery::SelfInstall
    } else {
        Delivery::NeedsAdmin
    }
}

/// Decide for the process running right now.
pub fn delivery() -> Delivery {
    delivery_from(
        cfg!(any(windows, target_os = "macos")),
        std::env::var("APPIMAGE").ok().as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_updater_endpoint_is_https() {
        // The plugin refuses a plain-http endpoint, and it refuses it during
        // initialisation — which takes the whole application down with it. The
        // window never opens, so the vault never opens, so a mistake in one
        // line of configuration costs somebody every second factor they own.
        //
        // Nothing else catches this. The test suite never builds a window and
        // CI never launches one, and the configuration is compiled into the
        // binary, so checking the file here is checking exactly what ships.
        let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json"))
            .expect("tauri.conf.json sits beside Cargo.toml");
        let config: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

        let endpoints = config["plugins"]["updater"]["endpoints"]
            .as_array()
            .expect("the updater needs somewhere to ask");
        assert!(
            !endpoints.is_empty(),
            "an updater with no endpoint asks nobody"
        );

        for endpoint in endpoints {
            let url = endpoint.as_str().expect("endpoints are strings");
            assert!(
                url.starts_with("https://"),
                "the updater refuses this at startup and the app will not open: {url}"
            );
        }
    }

    #[test]
    fn the_windows_installer_runs_again_over_the_top_unattended() {
        assert_eq!(delivery_from(true, None), Delivery::SelfInstall);
    }

    #[test]
    fn a_macos_app_bundle_is_the_users_own_and_needs_no_password() {
        // The updater swaps the .app folder in place; nothing on macOS owns
        // it the way dpkg owns a .deb, so there is no prompt to warn about.
        assert_eq!(delivery_from(true, None), Delivery::SelfInstall);
    }

    #[test]
    fn an_appimage_is_one_file_the_process_already_owns() {
        assert_eq!(
            delivery_from(false, Some("/home/someone/Apps/Tessera.AppImage")),
            Delivery::SelfInstall
        );
    }

    #[test]
    fn a_package_managed_install_warns_that_a_password_is_coming() {
        // A .deb or .rpm goes through pkexec, so the system asks for an
        // administrator password. Saying so first is the difference between a
        // prompt the user expected and one that teaches them to type root
        // passwords into surprises.
        assert_eq!(delivery_from(false, None), Delivery::NeedsAdmin);
    }

    #[test]
    fn an_empty_appimage_variable_is_not_an_appimage() {
        // An exported-but-empty variable is a shell leaving something behind,
        // not a runtime announcing itself. Believing it would promise a quiet
        // update and then produce a password prompt anyway.
        assert_eq!(delivery_from(false, Some("")), Delivery::NeedsAdmin);
    }
}
