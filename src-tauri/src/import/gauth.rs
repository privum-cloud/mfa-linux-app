//! Google Authenticator's export format.
//!
//! Google Authenticator's cloud sync has no public API — no third-party
//! application can read or write it. What it does offer is an export, which
//! produces one or more QR codes holding
//! `otpauth-migration://offline?data=<base64 protobuf>`. That payload is the
//! only documented way accounts leave the application, and Tessera both reads
//! and writes it, so accounts can travel in either direction.
//!
//! The schema, which cannot drift without breaking import in Google's own
//! application:
//!
//! ```text
//! MigrationPayload      OtpParameters
//!   1 repeated params     1 bytes  secret
//!   2 int32 version       2 string name
//!   3 int32 batch_size    3 string issuer
//!   4 int32 batch_index   4 enum   algorithm  1 SHA1, 2 SHA256, 3 SHA512, 4 MD5
//!   5 int32 batch_id      5 enum   digits     1 six, 2 eight
//!                         6 enum   type       1 HOTP, 2 TOTP
//!                         7 int64  counter
//! ```

use base64::Engine;
use url::Url;
use zeroize::Zeroizing;

use super::protobuf::{Reader, Writer, WIRE_LENGTH};
use super::ImportError;
use crate::model::{Account, AccountKind};
use crate::otp::{Algorithm, Secret};

/// Google splits an export across several QR codes when it has many accounts.
pub const ACCOUNTS_PER_BATCH: usize = 10;

/// Read a `otpauth-migration://offline?data=` payload into accounts.
pub fn parse_migration(uri: &str) -> Result<Vec<Account>, ImportError> {
    let url = Url::parse(uri).map_err(|_| ImportError::NotMigration)?;
    if url.scheme() != "otpauth-migration" {
        return Err(ImportError::NotMigration);
    }

    let data = url
        .query_pairs()
        .find(|(key, _)| key == "data")
        .map(|(_, value)| value.into_owned())
        .ok_or(ImportError::NotMigration)?;

    let bytes = decode_base64(&data).ok_or(ImportError::NotMigration)?;

    let mut accounts = Vec::new();
    let mut reader = Reader::new(&bytes);
    while !reader.is_empty() {
        let (field, wire) = reader.read_key().ok_or(ImportError::NotMigration)?;
        if field == 1 && wire == WIRE_LENGTH {
            let params = reader.read_bytes().ok_or(ImportError::NotMigration)?;
            accounts.push(read_parameters(params)?);
        } else {
            // Unknown fields are stepped over: the format may grow, and
            // refusing the whole payload would strand accounts on the phone.
            reader.skip(wire).ok_or(ImportError::NotMigration)?;
        }
    }
    Ok(accounts)
}

/// Write accounts back out as migration links, one per batch, for a phone to scan.
pub fn to_migration_uris(accounts: &[Account], per_batch: usize) -> Vec<Zeroizing<String>> {
    let per_batch = per_batch.max(1);
    let batches: Vec<_> = accounts.chunks(per_batch).collect();
    let batch_count = batches.len();

    batches
        .into_iter()
        .enumerate()
        .map(|(index, batch)| {
            let mut writer = Writer::new();
            for account in batch {
                writer.bytes_field(1, &write_parameters(account));
            }
            writer.varint_field(2, 1); // version
            writer.varint_field(3, batch_count as u64);
            writer.varint_field(4, index as u64);
            // batch_id ties the QR codes of one export together. Google uses a
            // random number; a constant is honest here because Tessera writes
            // every batch of an export in one go.
            writer.varint_field(5, 1);

            let encoded = base64::engine::general_purpose::STANDARD.encode(writer.finish());
            Zeroizing::new(format!("otpauth-migration://offline?data={encoded}"))
        })
        .collect()
}

fn read_parameters(bytes: &[u8]) -> Result<Account, ImportError> {
    let mut secret: Option<Secret> = None;
    let mut name = String::new();
    let mut issuer = String::new();
    let mut algorithm = Algorithm::Sha1;
    let mut digits = 6u32;
    let mut kind = AccountKind::Totp;
    let mut counter = 0u64;

    let mut reader = Reader::new(bytes);
    while !reader.is_empty() {
        let (field, wire) = reader.read_key().ok_or(ImportError::NotMigration)?;
        match (field, wire) {
            (1, WIRE_LENGTH) => {
                let raw = reader.read_bytes().ok_or(ImportError::NotMigration)?;
                if raw.is_empty() {
                    return Err(ImportError::MissingSecret);
                }
                secret = Some(Secret::from_bytes(raw.to_vec()));
            }
            (2, WIRE_LENGTH) => name = read_string(&mut reader)?,
            (3, WIRE_LENGTH) => issuer = read_string(&mut reader)?,
            (4, _) => {
                algorithm = match reader.read_varint().ok_or(ImportError::NotMigration)? {
                    // 0 means unspecified, which every exporter treats as SHA-1.
                    0 | 1 => Algorithm::Sha1,
                    2 => Algorithm::Sha256,
                    3 => Algorithm::Sha512,
                    // MD5 is in the schema and not in Tessera. Refusing is
                    // better than importing an account whose codes are wrong.
                    _ => return Err(ImportError::UnsupportedAlgorithm),
                };
            }
            (5, _) => {
                digits = match reader.read_varint().ok_or(ImportError::NotMigration)? {
                    2 => 8,
                    _ => 6,
                };
            }
            (6, _) => {
                kind = match reader.read_varint().ok_or(ImportError::NotMigration)? {
                    1 => AccountKind::Hotp,
                    _ => AccountKind::Totp,
                };
            }
            (7, _) => counter = reader.read_varint().ok_or(ImportError::NotMigration)?,
            (_, wire) => {
                reader.skip(wire).ok_or(ImportError::NotMigration)?;
            }
        }
    }

    // Google writes "Issuer:label" into `name` as well as filling `issuer`, and
    // older exports fill only the prefix. Take the dedicated field when it is
    // there and fall back to the prefix when it is not.
    let (prefix, label) = match name.split_once(':') {
        Some((prefix, label)) => (prefix.trim().to_owned(), label.trim().to_owned()),
        None => (String::new(), name.trim().to_owned()),
    };
    if issuer.is_empty() {
        issuer = prefix;
    }

    let mut account = Account::new(issuer, label, secret.ok_or(ImportError::MissingSecret)?);
    account.kind = kind;
    account.algorithm = algorithm;
    account.digits = digits;
    account.counter = counter;
    Ok(account)
}

fn read_string(reader: &mut Reader<'_>) -> Result<String, ImportError> {
    let raw = reader.read_bytes().ok_or(ImportError::NotMigration)?;
    // Lossy rather than fatal: a label with one bad byte is still an account
    // worth having, and the user can rename it.
    Ok(String::from_utf8_lossy(raw).into_owned())
}

fn write_parameters(account: &Account) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.bytes_field(1, account.secret.expose());

    let name = if account.issuer.is_empty() {
        account.label.clone()
    } else {
        format!("{}:{}", account.issuer, account.label)
    };
    writer.bytes_field(2, name.as_bytes());
    if !account.issuer.is_empty() {
        writer.bytes_field(3, account.issuer.as_bytes());
    }

    writer.varint_field(
        4,
        match account.algorithm {
            Algorithm::Sha1 => 1,
            Algorithm::Sha256 => 2,
            Algorithm::Sha512 => 3,
        },
    );
    writer.varint_field(5, if account.digits == 8 { 2 } else { 1 });
    writer.varint_field(
        6,
        match account.kind {
            AccountKind::Hotp => 1,
            // Steam has no representation in this schema. Exporting it as TOTP
            // is the closest true statement: the phone will generate six digits
            // where Steam wants five, which is visibly wrong rather than subtly
            // wrong.
            _ => 2,
        },
    );
    if account.kind == AccountKind::Hotp {
        writer.varint_field(7, account.counter);
    }

    writer.finish()
}

/// Decode base64 however the exporter happened to write it.
///
/// Payloads turn up percent-encoded, unpadded, and in the URL-safe alphabet
/// depending on which application produced the QR code.
fn decode_base64(data: &str) -> Option<Vec<u8>> {
    let normalised: String = data
        .replace("%2B", "+")
        .replace("%2F", "/")
        .replace("%3D", "=")
        .replace('-', "+")
        .replace('_', "/")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(normalised.trim_end_matches('='))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-tripping is the strongest test available without a phone in hand:
    /// build a payload with the writer, read it back, and require every field
    /// to survive. A mistake in either direction shows up as a mismatch.
    fn sample(issuer: &str, label: &str) -> Account {
        Account::new(
            issuer.into(),
            label.into(),
            Secret::from_bytes(b"12345678901234567890".to_vec()),
        )
    }

    #[test]
    fn round_trips_a_single_account() {
        let original = sample("GitHub", "you@example.com");
        let uris = to_migration_uris(std::slice::from_ref(&original), ACCOUNTS_PER_BATCH);
        assert_eq!(uris.len(), 1);

        let back = parse_migration(&uris[0]).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].issuer, "GitHub");
        assert_eq!(back[0].label, "you@example.com");
        assert_eq!(back[0].secret, original.secret);
        assert_eq!(back[0].kind, AccountKind::Totp);
    }

    #[test]
    fn round_trips_every_algorithm_and_digit_count() {
        let mut accounts = Vec::new();
        for (index, algorithm) in [Algorithm::Sha1, Algorithm::Sha256, Algorithm::Sha512]
            .into_iter()
            .enumerate()
        {
            let mut a = sample("Service", &format!("user{index}"));
            a.algorithm = algorithm;
            a.digits = if index % 2 == 0 { 6 } else { 8 };
            accounts.push(a);
        }

        let uris = to_migration_uris(&accounts, ACCOUNTS_PER_BATCH);
        let back = parse_migration(&uris[0]).unwrap();

        assert_eq!(back.len(), 3);
        for (original, restored) in accounts.iter().zip(back.iter()) {
            assert_eq!(restored.algorithm, original.algorithm, "algorithm was lost");
            assert_eq!(restored.digits, original.digits, "digit count was lost");
        }
    }

    #[test]
    fn round_trips_an_hotp_counter() {
        let mut account = sample("Bank", "personal");
        account.kind = AccountKind::Hotp;
        account.counter = 4242;

        let uris = to_migration_uris(std::slice::from_ref(&account), ACCOUNTS_PER_BATCH);
        let back = parse_migration(&uris[0]).unwrap();

        assert_eq!(back[0].kind, AccountKind::Hotp);
        assert_eq!(back[0].counter, 4242, "the counter was lost");
    }

    #[test]
    fn splits_a_long_export_into_batches_the_way_google_does() {
        let accounts: Vec<_> = (0..25)
            .map(|i| sample("Service", &format!("user{i}")))
            .collect();

        let uris = to_migration_uris(&accounts, ACCOUNTS_PER_BATCH);
        assert_eq!(
            uris.len(),
            3,
            "25 accounts should make three batches of ten"
        );

        let total: usize = uris.iter().map(|u| parse_migration(u).unwrap().len()).sum();
        assert_eq!(total, 25, "accounts were lost across the batches");
    }

    #[test]
    fn reads_a_name_that_carries_its_issuer_as_a_prefix() {
        // Google writes "Issuer:label" into `name` as well as filling `issuer`,
        // and older exports fill only the prefix.
        let mut params = Writer::new();
        params.bytes_field(1, b"12345678901234567890");
        params.bytes_field(2, b"Big Bank:alice@example.com");
        params.varint_field(6, 2);
        let mut writer = Writer::new();
        writer.bytes_field(1, &params.finish());

        let payload = base64::engine::general_purpose::STANDARD.encode(writer.finish());
        let back = parse_migration(&format!("otpauth-migration://offline?data={payload}")).unwrap();

        assert_eq!(back[0].issuer, "Big Bank");
        assert_eq!(back[0].label, "alice@example.com");
    }

    #[test]
    fn accepts_base64_however_the_exporter_encoded_it() {
        // Payloads appear percent-encoded, unpadded, and in the URL-safe
        // alphabet depending on who generated the QR code.
        let account = sample("Example", "alice");
        let canonical =
            to_migration_uris(std::slice::from_ref(&account), ACCOUNTS_PER_BATCH)[0].to_string();
        let data = canonical.split("data=").nth(1).unwrap().to_owned();

        let variants = [
            data.replace('+', "%2B")
                .replace('/', "%2F")
                .replace('=', "%3D"),
            data.trim_end_matches('=').to_owned(),
            data.replace('+', "-").replace('/', "_"),
        ];

        for variant in variants {
            let uri = format!("otpauth-migration://offline?data={variant}");
            assert_eq!(
                parse_migration(&uri).unwrap().len(),
                1,
                "failed on variant {variant}"
            );
        }
    }

    #[test]
    fn refuses_what_is_not_a_migration_payload() {
        assert_eq!(
            parse_migration("otpauth://totp/x?secret=GEZDGNBV"),
            Err(ImportError::NotMigration)
        );
        assert_eq!(
            parse_migration("otpauth-migration://offline"),
            Err(ImportError::NotMigration)
        );
        assert_eq!(
            parse_migration("otpauth-migration://offline?data=!!!not base64!!!"),
            Err(ImportError::NotMigration)
        );
    }

    #[test]
    fn refuses_md5_rather_than_importing_an_account_it_cannot_generate_codes_for() {
        // The schema allows MD5. Tessera does not implement it, and an account
        // that silently produces wrong codes is worse than one that was refused.
        let mut params = Writer::new();
        params.bytes_field(1, b"12345678901234567890");
        params.bytes_field(2, b"alice");
        params.varint_field(4, 4);
        params.varint_field(6, 2);
        let mut writer = Writer::new();
        writer.bytes_field(1, &params.finish());

        let payload = base64::engine::general_purpose::STANDARD.encode(writer.finish());
        assert_eq!(
            parse_migration(&format!("otpauth-migration://offline?data={payload}")),
            Err(ImportError::UnsupportedAlgorithm)
        );
    }

    #[test]
    fn ignores_fields_it_does_not_know() {
        // A future exporter may add fields. Refusing the whole payload over one
        // would strand the user's accounts on their phone.
        let mut params = Writer::new();
        params.bytes_field(1, b"12345678901234567890");
        params.bytes_field(2, b"alice");
        params.varint_field(6, 2);
        params.varint_field(99, 12345);
        let mut writer = Writer::new();
        writer.bytes_field(1, &params.finish());
        writer.varint_field(42, 7);

        let payload = base64::engine::general_purpose::STANDARD.encode(writer.finish());
        assert_eq!(
            parse_migration(&format!("otpauth-migration://offline?data={payload}"))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn an_empty_payload_is_an_empty_import_not_an_error() {
        let payload = base64::engine::general_purpose::STANDARD.encode(Writer::new().finish());
        assert_eq!(
            parse_migration(&format!("otpauth-migration://offline?data={payload}"))
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn a_full_batch_survives_being_drawn_as_a_qr_code_and_read_back() {
        // The end a phone actually sees: accounts become a payload, the payload
        // becomes a picture, and the picture has to come back as the same
        // accounts. Ten is a full Google batch — if that does not fit in one QR
        // code, exporting is broken for anyone with a normal number of accounts.
        let accounts: Vec<_> = (0..ACCOUNTS_PER_BATCH)
            .map(|i| sample("Some Service Ltd", &format!("person{i}@privum.cloud")))
            .collect();

        let uris = to_migration_uris(&accounts, ACCOUNTS_PER_BATCH);
        assert_eq!(uris.len(), 1, "a full batch should be one code");

        let png = super::super::render_qr_png(&uris[0]).expect("a full batch must fit in one QR");
        let read_back = super::super::read_qr_codes(&png).unwrap();
        assert_eq!(read_back.len(), 1);

        let restored = parse_migration(&read_back[0]).unwrap();
        assert_eq!(restored.len(), ACCOUNTS_PER_BATCH);
        for (original, back) in accounts.iter().zip(restored.iter()) {
            assert_eq!(
                back.secret, original.secret,
                "a secret was lost in the picture"
            );
            assert_eq!(back.issuer, original.issuer);
            assert_eq!(back.label, original.label);
        }
    }
}
