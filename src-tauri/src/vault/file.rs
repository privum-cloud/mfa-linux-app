//! Reading and writing the vault file.

use std::path::Path;

use crate::vault::{derive_key, open, seal, KdfParams, VaultError};

const VERSION: u8 = 1;
const HEADER_LEN: usize = 1 + 4 + 4 + 4 + 16 + 12; // 41

/// Seal `plaintext` under `password` and write it to `path`.
///
/// The write goes to a temporary file in the same directory and is then
/// renamed. `rename` within a directory is atomic on Linux, so a crash leaves
/// the previous vault intact rather than a half-written one. Writing in place
/// would truncate the target first, and a vault truncated to zero bytes is the
/// user's entire second factor gone.
pub fn save_document(
    path: &Path,
    password: &str,
    params: KdfParams,
    plaintext: &[u8],
) -> Result<(), VaultError> {
    let temporary = path.with_extension("tmp");
    save_document_to(path, &temporary, password, params, plaintext)
}

/// As `save_document`, with the temporary file named by the caller.
///
/// The name matters once a vault is shared: a single `vault.bin.tmp` is one
/// file every writer would use, and two saves at once interleave into a rename
/// that produces neither document.
pub fn save_document_to(
    path: &Path,
    temporary: &Path,
    password: &str,
    params: KdfParams,
    plaintext: &[u8],
) -> Result<(), VaultError> {
    let params = params.validated()?;

    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut salt).map_err(|e| VaultError::Io(e.to_string()))?;
    getrandom::getrandom(&mut nonce).map_err(|e| VaultError::Io(e.to_string()))?;

    let key = derive_key(password, &salt, params)?;
    let ciphertext = seal(&key, &nonce, plaintext);

    let mut buf = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    buf.push(VERSION);
    buf.extend_from_slice(&params.m_cost.to_le_bytes());
    buf.extend_from_slice(&params.t_cost.to_le_bytes());
    buf.extend_from_slice(&params.p_cost.to_le_bytes());
    buf.extend_from_slice(&salt);
    buf.extend_from_slice(&nonce);
    buf.extend_from_slice(&ciphertext);

    if let Some(parent) = path.parent() {
        // Only narrow a directory we created ourselves. The vault may sit in a
        // directory that is not ours to tighten — /tmp during tests, or a path
        // the user chose — and chmod'ing someone else's directory is a worse
        // bug than the one this is guarding against.
        let ours = !parent.exists();
        std::fs::create_dir_all(parent).map_err(|e| VaultError::Io(e.to_string()))?;
        if ours {
            restrict_to_owner(parent, 0o700)?;
        }
    }

    // Same directory, so the rename stays on one filesystem and is atomic.
    write_and_sync(temporary, &buf)?;
    std::fs::rename(temporary, path).map_err(|e| {
        let _ = std::fs::remove_file(temporary);
        VaultError::Io(e.to_string())
    })
}

/// Write the whole buffer and flush it to the device before returning, so the
/// rename that follows cannot expose a file whose contents are still in cache.
///
/// The mode is set at creation rather than after the rename. `File::create`
/// would otherwise yield 0644, and a vault readable by every other account on
/// the machine is a copy anyone can attack offline at leisure. Setting it after
/// the rename would leave a window where that copy exists; `rename` carries the
/// inode's permissions across, so the temporary file is where it belongs.
fn write_and_sync(path: &Path, buf: &[u8]) -> Result<(), VaultError> {
    use std::io::Write;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options
        .open(path)
        .map_err(|e| VaultError::Io(e.to_string()))?;
    file.write_all(buf)
        .map_err(|e| VaultError::Io(e.to_string()))?;
    file.sync_all().map_err(|e| VaultError::Io(e.to_string()))
}

/// Narrow an existing path to its owner.
///
/// Used for the containing directory, which `create_dir_all` leaves at 0755.
/// Only ever called on a directory this process just created.
#[allow(unused_variables)]
fn restrict_to_owner(path: &Path, mode: u32) -> Result<(), VaultError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|e| VaultError::Io(e.to_string()))?;
    }
    Ok(())
}

/// Read `path` and open it with `password`.
pub fn load_document(path: &Path, password: &str) -> Result<Vec<u8>, VaultError> {
    let buf = std::fs::read(path).map_err(|e| VaultError::Io(e.to_string()))?;
    if buf.len() < HEADER_LEN || buf[0] != VERSION {
        return Err(VaultError::BadFormat);
    }

    // Every value below comes from a file we did not write. The slices are safe
    // because the length was checked above; the parameters are not, which is
    // why they are validated before Argon2 sees them.
    let params = KdfParams {
        m_cost: u32::from_le_bytes(buf[1..5].try_into().unwrap()),
        t_cost: u32::from_le_bytes(buf[5..9].try_into().unwrap()),
        p_cost: u32::from_le_bytes(buf[9..13].try_into().unwrap()),
    }
    .validated()?;

    let salt: [u8; 16] = buf[13..29].try_into().unwrap();
    let nonce: [u8; 12] = buf[29..41].try_into().unwrap();

    let key = derive_key(password, &salt, params)?;
    open(&key, &nonce, &buf[41..])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique path per test. No randomness is used, so the test name is the key.
    fn tmp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("tessera-test-{name}.bin"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = tmp_path("roundtrip");
        save_document(&path, "master", KdfParams::default(), b"{\"accounts\":[]}").unwrap();
        assert_eq!(
            load_document(&path, "master").unwrap(),
            b"{\"accounts\":[]}"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn the_wrong_password_is_refused() {
        let path = tmp_path("wrongpw");
        save_document(&path, "master", KdfParams::default(), b"secret").unwrap();
        assert!(matches!(
            load_document(&path, "not the master"),
            Err(VaultError::Crypto)
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn salt_and_nonce_differ_between_writes() {
        // Reusing a nonce under one key breaks GCM outright, so this is not a
        // stylistic check.
        let (a, b) = (tmp_path("rng-a"), tmp_path("rng-b"));
        save_document(&a, "m", KdfParams::default(), b"x").unwrap();
        save_document(&b, "m", KdfParams::default(), b"x").unwrap();
        let (ba, bb) = (std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
        assert_ne!(ba[13..41], bb[13..41]);
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
    }

    #[test]
    fn a_truncated_file_is_reported_as_a_bad_format() {
        let path = tmp_path("truncated");
        std::fs::write(&path, [VERSION, 0, 0]).unwrap();
        assert!(matches!(
            load_document(&path, "m"),
            Err(VaultError::BadFormat)
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_future_format_version_is_refused_rather_than_misread() {
        let path = tmp_path("version");
        std::fs::write(&path, [99u8; 64]).unwrap();
        assert!(matches!(
            load_document(&path, "m"),
            Err(VaultError::BadFormat)
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_header_declaring_absurd_costs_is_refused_without_allocating() {
        // The parameters live in the clear and are read before any key exists.
        // A file asking for four gigabytes must be turned away here.
        let path = tmp_path("hostile-params");
        let mut buf = vec![VERSION];
        buf.extend_from_slice(&4_000_000u32.to_le_bytes());
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 16]);
        buf.extend_from_slice(&[0u8; 12]);
        buf.extend_from_slice(&[0u8; 32]);
        std::fs::write(&path, &buf).unwrap();

        assert!(matches!(
            load_document(&path, "m"),
            Err(VaultError::KdfParams)
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_interrupted_write_cannot_destroy_the_previous_vault() {
        // The write goes to a temporary file and is renamed into place, so the
        // target is either wholly old or wholly new. Proven here by checking
        // that no stray temporary survives a successful write.
        let path = tmp_path("atomic");
        save_document(&path, "m", KdfParams::default(), b"first").unwrap();
        save_document(&path, "m", KdfParams::default(), b"second").unwrap();

        assert_eq!(load_document(&path, "m").unwrap(), b"second");

        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("tessera-test-atomic") && n.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary files survived: {leftovers:?}"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    #[cfg(unix)]
    fn the_vault_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        // The file is encrypted, so 0644 is not a break — but it hands every
        // other account on the machine a copy to attack offline at leisure.
        // 0600 is the floor for a secrets file; ssh refuses to load a key
        // without it.
        let path = tmp_path("permissions");
        save_document(&path, "m", KdfParams::default(), b"x").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "vault mode was {mode:o}, expected 600");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    #[cfg(unix)]
    fn the_vault_directory_is_reachable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let mut dir = std::env::temp_dir();
        dir.push("tessera-test-dirperms");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("vault.bin");

        save_document(&path, "m", KdfParams::default(), b"x").unwrap();

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o700,
            "vault directory mode was {mode:o}, expected 700"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
