//! The `otpauth://` URI, which is what a service's QR code actually contains.
//!
//! The format is Google's Key Uri Format. It is loosely followed in the wild,
//! so the parser is forgiving about what it accepts and strict about what it
//! emits.

use url::Url;
use zeroize::Zeroizing;

use crate::model::{Account, AccountKind};
use crate::otp::{Algorithm, Secret};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImportError {
    #[error("that is not an otpauth:// link")]
    NotOtpauth,
    #[error("Tessera does not know that kind of one-time password")]
    UnknownKind,
    #[error("the link has no secret in it")]
    MissingSecret,
    #[error("the secret in the link is not valid base32")]
    BadSecret,
    #[error("the link has an unusable {0}")]
    BadParameter(String),
}

/// Digit counts outside this range cannot produce a code anyone can type.
const MIN_DIGITS: u32 = 4;
const MAX_DIGITS: u32 = 10;

/// Read an `otpauth://` link into an account.
pub fn parse_otpauth(uri: &str) -> Result<Account, ImportError> {
    let url = Url::parse(uri).map_err(|_| ImportError::NotOtpauth)?;
    if url.scheme() != "otpauth" {
        return Err(ImportError::NotOtpauth);
    }

    let kind = match url.host_str() {
        Some("totp") => AccountKind::Totp,
        Some("hotp") => AccountKind::Hotp,
        Some("steam") => AccountKind::Steam,
        _ => return Err(ImportError::UnknownKind),
    };

    // `Url::path` hands back the still-encoded `/Issuer:label`.
    let decoded = percent_decode(url.path().trim_start_matches('/'));
    let (issuer_from_label, label) = match decoded.split_once(':') {
        Some((issuer, label)) => (issuer.trim().to_owned(), label.trim().to_owned()),
        None => (String::new(), decoded.trim().to_owned()),
    };

    let mut secret = None;
    // The label prefix is a fallback; the format says the parameter wins.
    let mut issuer = issuer_from_label;
    let mut algorithm = Algorithm::Sha1;
    let mut digits = if kind == AccountKind::Steam { 5 } else { 6 };
    let mut period = 30u32;
    let mut counter = 0u64;

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "secret" => {
                secret = Some(Secret::from_base32(&value).map_err(|_| ImportError::BadSecret)?)
            }
            "issuer" => issuer = value.trim().to_owned(),
            "algorithm" => {
                algorithm = match value.to_ascii_uppercase().as_str() {
                    "SHA1" => Algorithm::Sha1,
                    "SHA256" => Algorithm::Sha256,
                    "SHA512" => Algorithm::Sha512,
                    _ => return Err(ImportError::BadParameter("algorithm".into())),
                }
            }
            "digits" => {
                digits = value
                    .parse()
                    .ok()
                    .filter(|d| (MIN_DIGITS..=MAX_DIGITS).contains(d))
                    .ok_or_else(|| ImportError::BadParameter("digit count".into()))?
            }
            "period" => {
                period = value
                    .parse()
                    .ok()
                    .filter(|p| *p > 0)
                    .ok_or_else(|| ImportError::BadParameter("period".into()))?
            }
            "counter" => {
                counter = value
                    .parse()
                    .map_err(|_| ImportError::BadParameter("counter".into()))?
            }
            // Unknown parameters are ignored rather than refused: the format
            // grows, and a link Tessera cannot fully describe is still usable.
            _ => {}
        }
    }

    let mut account = Account::new(issuer, label, secret.ok_or(ImportError::MissingSecret)?);
    account.kind = kind;
    account.algorithm = algorithm;
    account.digits = digits;
    account.period = period;
    account.counter = counter;
    Ok(account)
}

/// Write an account back out as a link, for export and for showing a QR code.
///
/// The result carries the secret, so it is wrapped in `Zeroizing`.
pub fn to_otpauth(account: &Account) -> Zeroizing<String> {
    let kind = match account.kind {
        AccountKind::Totp => "totp",
        AccountKind::Hotp => "hotp",
        AccountKind::Steam => "steam",
    };
    let algorithm = match account.algorithm {
        Algorithm::Sha1 => "SHA1",
        Algorithm::Sha256 => "SHA256",
        Algorithm::Sha512 => "SHA512",
    };

    let label = if account.issuer.is_empty() {
        encode(&account.label)
    } else {
        format!("{}:{}", encode(&account.issuer), encode(&account.label))
    };

    let mut uri = format!(
        "otpauth://{kind}/{label}?secret={}&algorithm={algorithm}&digits={}",
        &*account.secret.to_base32(),
        account.digits
    );
    if !account.issuer.is_empty() {
        uri.push_str(&format!("&issuer={}", encode(&account.issuer)));
    }
    match account.kind {
        AccountKind::Hotp => uri.push_str(&format!("&counter={}", account.counter)),
        _ => uri.push_str(&format!("&period={}", account.period)),
    }
    Zeroizing::new(uri)
}

/// Percent-encode a label component. Only the characters that would change how
/// the URI parses are escaped.
fn encode(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            ':' => "%3A".to_owned(),
            '/' => "%2F".to_owned(),
            '?' => "%3F".to_owned(),
            '#' => "%23".to_owned(),
            '&' => "%26".to_owned(),
            ' ' => "%20".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

/// Decode the percent escapes `Url` leaves in a path segment.
fn percent_decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn reads_the_shape_a_service_actually_issues() {
        let account = parse_otpauth(&format!(
            "otpauth://totp/GitHub:you@example.com?secret={SECRET_B32}&issuer=GitHub"
        ))
        .unwrap();

        assert_eq!(account.issuer, "GitHub");
        assert_eq!(account.label, "you@example.com");
        assert_eq!(account.kind, AccountKind::Totp);
        assert_eq!(account.secret.expose(), b"12345678901234567890");
        // Absent parameters take the defaults the format specifies.
        assert_eq!(account.algorithm, Algorithm::Sha1);
        assert_eq!(account.digits, 6);
        assert_eq!(account.period, 30);
    }

    #[test]
    fn reads_every_optional_parameter() {
        let account = parse_otpauth(&format!(
            "otpauth://totp/Example:alice?secret={SECRET_B32}&algorithm=SHA512&digits=8&period=60"
        ))
        .unwrap();
        assert_eq!(account.algorithm, Algorithm::Sha512);
        assert_eq!(account.digits, 8);
        assert_eq!(account.period, 60);
    }

    #[test]
    fn reads_an_hotp_counter() {
        let account = parse_otpauth(&format!(
            "otpauth://hotp/Bank:alice?secret={SECRET_B32}&counter=42"
        ))
        .unwrap();
        assert_eq!(account.kind, AccountKind::Hotp);
        assert_eq!(account.counter, 42);
    }

    #[test]
    fn decodes_percent_encoding_in_the_label() {
        // Issuers with spaces are ordinary, and the colon separator is encoded.
        let account = parse_otpauth(&format!(
            "otpauth://totp/Big%20Bank%3Aalice%40example.com?secret={SECRET_B32}"
        ))
        .unwrap();
        assert_eq!(account.issuer, "Big Bank");
        assert_eq!(account.label, "alice@example.com");
    }

    #[test]
    fn prefers_the_issuer_parameter_over_the_label_prefix() {
        // The format says the parameter wins when the two disagree.
        let account = parse_otpauth(&format!(
            "otpauth://totp/Stale:alice?secret={SECRET_B32}&issuer=Current"
        ))
        .unwrap();
        assert_eq!(account.issuer, "Current");
    }

    #[test]
    fn copes_with_a_label_that_has_no_issuer_at_all() {
        let account = parse_otpauth(&format!("otpauth://totp/alice?secret={SECRET_B32}")).unwrap();
        assert_eq!(account.issuer, "");
        assert_eq!(account.label, "alice");
    }

    #[test]
    fn refuses_what_it_cannot_generate_codes_for() {
        assert_eq!(
            parse_otpauth("https://example.com"),
            Err(ImportError::NotOtpauth)
        );
        assert_eq!(
            parse_otpauth(&format!("otpauth://yubico/x?secret={SECRET_B32}")),
            Err(ImportError::UnknownKind)
        );
        assert_eq!(
            parse_otpauth("otpauth://totp/alice"),
            Err(ImportError::MissingSecret)
        );
        assert_eq!(
            parse_otpauth("otpauth://totp/alice?secret=not!base32"),
            Err(ImportError::BadSecret)
        );
    }

    #[test]
    fn refuses_a_digit_count_that_cannot_produce_a_usable_code() {
        // hotp() no longer panics on a large digit count, but a 40-digit code
        // is still nonsense and is better refused at the door than shown.
        assert_eq!(
            parse_otpauth(&format!(
                "otpauth://totp/alice?secret={SECRET_B32}&digits=40"
            )),
            Err(ImportError::BadParameter("digit count".into()))
        );
        assert_eq!(
            parse_otpauth(&format!(
                "otpauth://totp/alice?secret={SECRET_B32}&period=0"
            )),
            Err(ImportError::BadParameter("period".into()))
        );
    }

    #[test]
    fn round_trips_through_its_own_output() {
        let original = parse_otpauth(&format!(
            "otpauth://totp/GitHub:you@example.com?secret={SECRET_B32}&issuer=GitHub&digits=8&period=60&algorithm=SHA256"
        ))
        .unwrap();
        let back = parse_otpauth(&to_otpauth(&original)).unwrap();

        assert_eq!(back.issuer, original.issuer);
        assert_eq!(back.label, original.label);
        assert_eq!(back.secret, original.secret);
        assert_eq!(back.algorithm, original.algorithm);
        assert_eq!(back.digits, original.digits);
        assert_eq!(back.period, original.period);
    }
}
