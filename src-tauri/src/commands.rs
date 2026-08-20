//! The Tauri command surface.
//!
//! Commands return generated codes and metadata. A raw secret never travels in
//! this direction — the front end has no need of one and no way to hold it
//! safely.

use serde::Serialize;

use crate::model::AccountKind;
use crate::otp::{seconds_remaining, steam_at, totp_at, Algorithm, Secret};

/// What the interface needs in order to render one row.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CodeView {
    pub code: String,
    pub seconds_remaining: u32,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the system clock is set before 1970")
        .as_secs()
}

/// Generate a code at an explicit moment.
///
/// Split out from the command so the clock can be supplied in tests; the
/// command itself is the same call with the real time.
fn code_at(
    secret: &str,
    kind: AccountKind,
    algorithm: Algorithm,
    digits: u32,
    period: u32,
    counter: u64,
    unix_seconds: u64,
) -> Result<CodeView, String> {
    let secret = Secret::from_base32(secret).map_err(|e| e.to_string())?;

    let (code, remaining) = match kind {
        AccountKind::Totp => (
            totp_at(algorithm, secret.expose(), unix_seconds, period, digits),
            seconds_remaining(unix_seconds, period),
        ),
        // An HOTP code stands until the user asks for the next one, so there is
        // no countdown to report.
        AccountKind::Hotp => (
            crate::otp::hotp(algorithm, secret.expose(), counter, digits),
            0,
        ),
        // Steam fixes its own shape: five characters over thirty seconds. The
        // account's `digits` and `period` are deliberately ignored here.
        AccountKind::Steam => (
            steam_at(secret.expose(), unix_seconds),
            seconds_remaining(unix_seconds, 30),
        ),
    };

    Ok(CodeView {
        code,
        seconds_remaining: remaining,
    })
}

/// Generate the code for an account as it stands right now.
#[tauri::command]
pub fn preview_code(
    secret: String,
    kind: AccountKind,
    algorithm: Algorithm,
    digits: u32,
    period: u32,
    counter: u64,
) -> Result<CodeView, String> {
    code_at(
        &secret,
        kind,
        algorithm,
        digits,
        period,
        counter,
        now_unix(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn generates_a_totp_code_matching_the_rfc_vector() {
        let view = code_at(RFC_BASE32, AccountKind::Totp, Algorithm::Sha1, 8, 30, 0, 59).unwrap();
        assert_eq!(view.code, "94287082");
        assert_eq!(view.seconds_remaining, 1);
    }

    #[test]
    fn generates_an_hotp_code_from_the_counter_not_the_clock() {
        let view = code_at(RFC_BASE32, AccountKind::Hotp, Algorithm::Sha1, 6, 30, 1, 59).unwrap();
        assert_eq!(view.code, "287082");
        // An HOTP code does not expire, so there is nothing to count down.
        assert_eq!(view.seconds_remaining, 0);
    }

    #[test]
    fn generates_a_steam_code() {
        let view = code_at(
            RFC_BASE32,
            AccountKind::Steam,
            Algorithm::Sha1,
            5,
            30,
            0,
            59,
        )
        .unwrap();
        assert_eq!(view.code.len(), 5);
    }

    #[test]
    fn reports_a_bad_secret_as_an_error_rather_than_panicking() {
        let result = code_at(
            "not base32!",
            AccountKind::Totp,
            Algorithm::Sha1,
            6,
            30,
            0,
            59,
        );
        assert!(result.is_err());
    }
}
