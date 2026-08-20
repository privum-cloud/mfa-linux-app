//! Steam's five-character variant of TOTP.
//!
//! Steam uses the standard HMAC-SHA1 dynamic truncation, then renders the
//! result in base 26 over its own alphabet instead of base 10. The alphabet
//! omits characters that are easy to misread aloud or on screen.

use super::algorithm::{mac, truncate};
use super::Algorithm;

const ALPHABET: &[u8] = b"23456789BCDFGHJKMNPQRTVWXY";
const CODE_LENGTH: usize = 5;

/// Steam's five-character code at a given moment. Always HMAC-SHA1 over a
/// 30-second period; Steam does not offer the choice.
pub fn steam_at(key: &[u8], unix_seconds: u64) -> String {
    let counter = unix_seconds / 30;
    let mut value = truncate(&mac(Algorithm::Sha1, key, &counter.to_be_bytes()));

    let mut code = String::with_capacity(CODE_LENGTH);
    for _ in 0..CODE_LENGTH {
        code.push(ALPHABET[value as usize % ALPHABET.len()] as char);
        value /= ALPHABET.len() as u32;
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"12345678901234567890";

    #[test]
    fn produces_five_characters_from_the_steam_alphabet() {
        for time in [0u64, 59, 1111111109, 2000000000] {
            let code = steam_at(KEY, time);
            assert_eq!(code.len(), CODE_LENGTH, "wrong length at t={time}");
            assert!(
                code.bytes().all(|b| ALPHABET.contains(&b)),
                "code {code} at t={time} used a character outside the alphabet"
            );
        }
    }

    #[test]
    fn alphabet_excludes_characters_that_are_easy_to_misread() {
        for &confusable in b"01IOSAEU" {
            assert!(
                !ALPHABET.contains(&confusable),
                "{} should not be in the Steam alphabet",
                confusable as char
            );
        }
    }

    #[test]
    fn is_stable_within_a_period_and_changes_across_one() {
        assert_eq!(steam_at(KEY, 30), steam_at(KEY, 59));
        assert_ne!(steam_at(KEY, 59), steam_at(KEY, 60));
    }
}
