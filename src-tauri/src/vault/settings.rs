//! Preferences the user can change.
//!
//! These live inside the sealed document rather than in a config file: they are
//! nobody's business but the user's, and once synchronisation exists they
//! travel between machines without any extra plumbing.

use serde::{Deserialize, Serialize};

/// Five minutes is long enough not to nag and short enough to matter on a
/// laptop left open in a café.
const DEFAULT_IDLE_TIMEOUT_SECS: u32 = 300;
const MIN_IDLE_TIMEOUT_SECS: u32 = 15;
const MAX_IDLE_TIMEOUT_SECS: u32 = 86_400;

/// Long enough to switch windows and paste, short enough that a forgotten code
/// does not sit in the clipboard all afternoon.
const DEFAULT_CLIPBOARD_CLEAR_SECS: u32 = 20;
const MIN_CLIPBOARD_CLEAR_SECS: u32 = 5;
const MAX_CLIPBOARD_CLEAR_SECS: u32 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub idle_timeout_secs: u32,
    pub clipboard_clear_secs: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
            clipboard_clear_secs: DEFAULT_CLIPBOARD_CLEAR_SECS,
        }
    }
}

impl Settings {
    /// Clamp rather than reject.
    ///
    /// These values come out of the vault document, which may have been edited
    /// by hand or written by a future version. Refusing the whole document over
    /// a bad preference would lock the user out of their accounts; clamping
    /// costs them a setting they can change back.
    pub fn validated(self) -> Self {
        Self {
            idle_timeout_secs: self
                .idle_timeout_secs
                .clamp(MIN_IDLE_TIMEOUT_SECS, MAX_IDLE_TIMEOUT_SECS),
            clipboard_clear_secs: self
                .clipboard_clear_secs
                .clamp(MIN_CLIPBOARD_CLEAR_SECS, MAX_CLIPBOARD_CLEAR_SECS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_documented_ones() {
        let s = Settings::default();
        assert_eq!(s.idle_timeout_secs, 300);
        assert_eq!(s.clipboard_clear_secs, 20);
    }

    #[test]
    fn values_are_clamped_rather_than_refused() {
        // These arrive from the vault document, so a hand-edited or synced file
        // can carry nonsense. A zero timeout would lock the vault between the
        // unlock and the first keystroke, which looks like the app is broken.
        let absurd = Settings {
            idle_timeout_secs: 0,
            clipboard_clear_secs: 0,
        }
        .validated();
        assert_eq!(absurd.idle_timeout_secs, MIN_IDLE_TIMEOUT_SECS);
        assert_eq!(absurd.clipboard_clear_secs, MIN_CLIPBOARD_CLEAR_SECS);

        let enormous = Settings {
            idle_timeout_secs: u32::MAX,
            clipboard_clear_secs: u32::MAX,
        }
        .validated();
        assert_eq!(enormous.idle_timeout_secs, MAX_IDLE_TIMEOUT_SECS);
        assert_eq!(enormous.clipboard_clear_secs, MAX_CLIPBOARD_CLEAR_SECS);
    }

    #[test]
    fn a_sensible_value_passes_through_untouched() {
        let chosen = Settings {
            idle_timeout_secs: 60,
            clipboard_clear_secs: 45,
        };
        assert_eq!(chosen.validated(), chosen);
    }
}
