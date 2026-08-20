//! One-time-password generation.
//!
//! This module performs no I/O and holds no state: it turns a secret and a
//! counter into a string of digits. That isolation is deliberate — it is the
//! most correctness-critical code in Tessera and also the easiest to test
//! exhaustively, because the RFCs publish their own expected outputs.

mod algorithm;
mod secret;
mod totp;

pub use algorithm::Algorithm;
pub use secret::{OtpError, Secret};
pub use totp::{seconds_remaining, totp_at};

use algorithm::{mac, truncate};

/// RFC 4226 counter-based one-time password.
pub fn hotp(alg: Algorithm, key: &[u8], counter: u64, digits: u32) -> String {
    let binary = truncate(&mac(alg, key, &counter.to_be_bytes()));
    let modulus = 10u32.pow(digits);
    format!("{:0width$}", binary % modulus, width = digits as usize)
}
