//! One-time-password generation.
//!
//! This module performs no I/O and holds no state: it turns a secret and a
//! counter into a string of digits. That isolation is deliberate — it is the
//! most correctness-critical code in Tessera and also the easiest to test
//! exhaustively, because the RFCs publish their own expected outputs.

mod algorithm;
mod secret;
mod steam;
mod totp;

pub use algorithm::Algorithm;
pub use secret::{OtpError, Secret};
pub use steam::steam_at;
pub use totp::{seconds_remaining, totp_at};

use algorithm::{mac, truncate};

/// RFC 4226 counter-based one-time password.
pub fn hotp(alg: Algorithm, key: &[u8], counter: u64, digits: u32) -> String {
    let binary = truncate(&mac(alg, key, &counter.to_be_bytes()));
    // u64 and capped, because `digits` is not ours to trust: it is deserialised
    // from the vault document and, once importing exists, from a Google
    // Authenticator protobuf we did not write. 10^10 overflows a u32 and 10^20
    // overflows a u64, and an overflow here is a panic in debug builds.
    let modulus = 10u64.pow(digits.min(19));
    format!(
        "{:0width$}",
        u64::from(binary) % modulus,
        width = digits as usize
    )
}
