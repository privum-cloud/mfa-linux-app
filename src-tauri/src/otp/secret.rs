//! The shared secret behind an account, held so it cannot be left in memory.

use base32::Alphabet;
use zeroize::Zeroizing;

/// Errors from parsing OTP inputs.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OtpError {
    #[error("the secret is not valid base32")]
    InvalidSecret,
}

/// A shared secret. The inner bytes are zeroed when dropped.
///
/// `Debug` is implemented by hand so a stray `{:?}` in a log line cannot print
/// the secret — the derived implementation would.
#[derive(Clone)]
pub struct Secret(Zeroizing<Vec<u8>>);

impl Secret {
    /// Parse a base32 secret as a service prints it.
    ///
    /// Whitespace is stripped, case is normalised, and `=` padding is dropped:
    /// services disagree about all three, and the user pastes what they see.
    pub fn from_base32(input: &str) -> Result<Self, OtpError> {
        let normalised: String = input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '=' && *c != '-')
            .collect::<String>()
            .to_ascii_uppercase();

        let bytes = base32::decode(Alphabet::Rfc4648 { padding: false }, &normalised)
            .ok_or(OtpError::InvalidSecret)?;

        if bytes.is_empty() {
            return Err(OtpError::InvalidSecret);
        }
        Ok(Self(Zeroizing::new(bytes)))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Borrow the raw bytes. Named to make call sites conspicuous in review.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// Re-encode as base32, for export and for showing a QR code.
    pub fn to_base32(&self) -> Zeroizing<String> {
        Zeroizing::new(base32::encode(
            Alphabet::Rfc4648 { padding: false },
            &self.0,
        ))
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret({} bytes, redacted)", self.0.len())
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Secret {}

impl serde::Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_base32())
    }
}

impl<'de> serde::Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        Secret::from_base32(&encoded).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The RFC 4226 secret `12345678901234567890` in base32.
    const RFC_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    const RFC_BYTES: &[u8] = b"12345678901234567890";

    #[test]
    fn decodes_rfc_secret_from_base32() {
        let secret = Secret::from_base32(RFC_BASE32).unwrap();
        assert_eq!(secret.expose(), RFC_BYTES);
    }

    #[test]
    fn tolerates_the_way_services_actually_print_secrets() {
        // Services show secrets lowercased, space-separated, and padded. All
        // three must decode to the same bytes, because users paste what they see.
        for input in [
            "gezdgnbvgy3tqojqgezdgnbvgy3tqojq",
            "GEZD GNBV GY3T QOJQ GEZD GNBV GY3T QOJQ",
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ====",
        ] {
            assert_eq!(
                Secret::from_base32(input).unwrap().expose(),
                RFC_BYTES,
                "failed to decode {input:?}"
            );
        }
    }

    #[test]
    fn rejects_non_base32() {
        assert_eq!(
            Secret::from_base32("not base32!"),
            Err(OtpError::InvalidSecret)
        );
    }

    #[test]
    fn rejects_an_empty_secret() {
        // An empty secret decodes cleanly as zero bytes but would generate a
        // code that is constant forever, which is worse than refusing it.
        assert_eq!(Secret::from_base32(""), Err(OtpError::InvalidSecret));
    }

    #[test]
    fn round_trips_through_base32() {
        let secret = Secret::from_bytes(RFC_BYTES.to_vec());
        assert_eq!(&*secret.to_base32(), RFC_BASE32);
    }

    #[test]
    fn debug_does_not_leak_the_secret() {
        let rendered = format!("{:?}", Secret::from_bytes(RFC_BYTES.to_vec()));
        assert!(
            !rendered.contains("12345"),
            "Debug leaked the secret: {rendered}"
        );
    }
}
