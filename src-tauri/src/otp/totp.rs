//! RFC 6238 time-based one-time passwords.

use super::{hotp, Algorithm};

/// RFC 6238 time-based one-time password at a given moment.
///
/// `unix_seconds` is passed in rather than read from the clock so the function
/// stays pure and the RFC vectors can be replayed exactly.
pub fn totp_at(alg: Algorithm, key: &[u8], unix_seconds: u64, period: u32, digits: u32) -> String {
    hotp(alg, key, unix_seconds / period as u64, digits)
}

/// Seconds until the current code expires. Returns a full period exactly on a
/// boundary, because that is the moment a fresh code has just begun.
pub fn seconds_remaining(unix_seconds: u64, period: u32) -> u32 {
    period - (unix_seconds % period as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 Appendix B. The secret is the RFC 4226 secret repeated to reach
    // each hash's block size, which is why the three differ in length.
    const SHA1_KEY: &[u8] = b"12345678901234567890";
    const SHA256_KEY: &[u8] = b"12345678901234567890123456789012";
    const SHA512_KEY: &[u8] = b"1234567890123456789012345678901234567890123456789012345678901234";

    #[test]
    fn rfc6238_appendix_b_vectors() {
        // (unix time, SHA-1, SHA-256, SHA-512) at 8 digits, T0 = 0, X = 30.
        let cases: &[(u64, &str, &str, &str)] = &[
            (59, "94287082", "46119246", "90693936"),
            (1111111109, "07081804", "68084774", "25091201"),
            (1111111111, "14050471", "67062674", "99943326"),
            (1234567890, "89005924", "91819424", "93441116"),
            (2000000000, "69279037", "90698825", "38618901"),
            // Past 2^31 seconds — catches a 32-bit counter.
            (20000000000, "65353130", "77737706", "47863826"),
        ];

        for (time, sha1, sha256, sha512) in cases {
            assert_eq!(
                &totp_at(Algorithm::Sha1, SHA1_KEY, *time, 30, 8),
                sha1,
                "SHA-1 diverged at t={time}"
            );
            assert_eq!(
                &totp_at(Algorithm::Sha256, SHA256_KEY, *time, 30, 8),
                sha256,
                "SHA-256 diverged at t={time}"
            );
            assert_eq!(
                &totp_at(Algorithm::Sha512, SHA512_KEY, *time, 30, 8),
                sha512,
                "SHA-512 diverged at t={time}"
            );
        }
    }

    #[test]
    fn code_is_stable_across_a_period_and_changes_at_the_boundary() {
        let at = |t| totp_at(Algorithm::Sha1, SHA1_KEY, t, 30, 6);
        assert_eq!(at(30), at(59), "code changed inside a single period");
        assert_ne!(at(59), at(60), "code did not change at the period boundary");
    }

    #[test]
    fn honours_a_non_default_period() {
        // Some services issue 60-second tokens. At t=59 a 60-second token is
        // still in its first period, where a 30-second token is in its second.
        assert_eq!(
            totp_at(Algorithm::Sha1, SHA1_KEY, 59, 60, 6),
            hotp(Algorithm::Sha1, SHA1_KEY, 0, 6)
        );
    }

    #[test]
    fn seconds_remaining_counts_down_to_the_boundary() {
        assert_eq!(seconds_remaining(0, 30), 30);
        assert_eq!(seconds_remaining(1, 30), 29);
        assert_eq!(seconds_remaining(29, 30), 1);
        assert_eq!(seconds_remaining(30, 30), 30);
    }
}
