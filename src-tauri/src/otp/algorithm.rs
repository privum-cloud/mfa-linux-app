//! HMAC dispatch and the dynamic truncation shared by HOTP and TOTP.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

/// The hash backing the HMAC. SHA-1 is the default because it is what nearly
/// every service issues, and what Google Authenticator assumes when the
/// `algorithm` parameter is absent from an `otpauth://` URI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum Algorithm {
    #[default]
    Sha1,
    Sha256,
    Sha512,
}

type HmacSha1 = Hmac<Sha1>;
type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

/// Compute HMAC(key, message) under the given hash.
///
/// `new_from_slice` only fails for algorithms with a fixed key size; all three
/// HMAC constructions here accept any key length, so the error is unreachable.
pub(crate) fn mac(alg: Algorithm, key: &[u8], message: &[u8]) -> Vec<u8> {
    match alg {
        Algorithm::Sha1 => {
            let mut m =
                <HmacSha1 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
            m.update(message);
            m.finalize().into_bytes().to_vec()
        }
        Algorithm::Sha256 => {
            let mut m =
                <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
            m.update(message);
            m.finalize().into_bytes().to_vec()
        }
        Algorithm::Sha512 => {
            let mut m =
                <HmacSha512 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
            m.update(message);
            m.finalize().into_bytes().to_vec()
        }
    }
}

/// Dynamic truncation, RFC 4226 section 5.3.
///
/// The low nibble of the final byte selects a four-byte window; the top bit of
/// that window is masked off so the result is positive in languages without
/// unsigned integers. Every digest here is at least 20 bytes, so an offset of
/// at most 15 plus three can never run past the end.
pub(crate) fn truncate(digest: &[u8]) -> u32 {
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::super::hotp;
    use super::Algorithm;

    /// RFC 4226 Appendix D uses this ASCII secret throughout.
    const SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn rfc4226_appendix_d_vectors() {
        let expected = [
            "755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583",
            "399871", "520489",
        ];
        for (counter, want) in expected.iter().enumerate() {
            assert_eq!(
                &hotp(Algorithm::Sha1, SECRET, counter as u64, 6),
                want,
                "HOTP diverged from RFC 4226 at counter {counter}"
            );
        }
    }

    #[test]
    fn digits_are_left_padded_with_zeroes() {
        // Counter 1 of the RFC vector is 287082; asking for 8 digits must not
        // truncate or right-align it.
        let code = hotp(Algorithm::Sha1, SECRET, 1, 8);
        assert_eq!(code.len(), 8);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn a_large_digit_count_does_not_panic() {
        // `digits` is not ours to trust: it is deserialised from the vault
        // document and, once importing exists, from a Google Authenticator
        // protobuf we did not write. 10^10 overflows a u32 and panics in debug.
        let code = hotp(Algorithm::Sha1, SECRET, 0, 10);
        assert_eq!(code.len(), 10);
    }
}
