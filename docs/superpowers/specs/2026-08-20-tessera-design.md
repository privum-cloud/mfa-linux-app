# Tessera — Design Specification

**Project:** Tessera — an open-source authenticator for Linux
**Repository:** `privum-cloud/mfa-linux-app`
**Application identifier:** `cloud.privum.tessera`
**License:** GPL-3.0-only
**Date:** 2026-08-20
**Status:** Design approved, pending spec review

---

## 1. Overview

Tessera is a desktop authenticator for Linux. It stores TOTP and HOTP secrets in a
locally encrypted vault, generates one-time codes, and synchronises that vault
across the user's machines through their Google account.

The name is Latin: a *tessera* was the token a Roman carried to prove who he was.

### 1.1 Goals

- Generate correct TOTP and HOTP codes for any account a user holds today.
- Import existing accounts from Google Authenticator, and export back to it.
- Encrypt everything at rest under a master password the user alone holds.
- Synchronise the encrypted vault between the user's machines via Google Drive.
- Provide the ordinary operations: add, copy, edit, delete, search, group.
- Ship as a normal Linux application: deb, rpm, AppImage, and Flatpak.

### 1.2 Non-goals

- **Reading Google Authenticator's cloud sync.** Not possible; see section 2.
- A hosted sync service of our own. Google Drive fills that role.
- Mobile applications. A future Android client could join the same vault format,
  but nothing in this specification depends on one existing.
- Push-based or approval-based second factors (Duo-style). TOTP and HOTP only.

### 1.3 Prior art, stated honestly

Ente Auth and GNOME Authenticator already run on Linux. Tessera is not the first
authenticator for the platform, and the README must not claim otherwise. The
differentiators are direct Google Authenticator import and export, synchronisation
through the user's own Google account, and the Privum interface.

---

## 2. The constraint that shapes the product

Google Authenticator's cloud sync has **no public API**. No third-party application
can read from or write to it. This was re-verified on 2026-08-20 and is a closed
Google-internal service, not an oversight we can work around.

What makes the product work anyway is a property of TOTP itself: **seeds are static**.
Two applications holding the same seed produce identical codes forever, with nothing
running between them. Synchronisation is only needed when an account is *added* or
*deleted* — never to keep codes agreeing.

That gives two usable paths:

1. **Google Authenticator to Tessera.** Its export produces
   `otpauth-migration://offline?data=<base64 protobuf>` QR codes. The protobuf schema
   is public and widely implemented. Tessera decodes them.
2. **Tessera to Google Authenticator.** Tessera encodes the same format, producing QR
   codes Google Authenticator will scan and import.

Both are manual and operate in batches. Neither is live sync, and neither needs to be.

**Signing in with Google therefore pulls the Tessera vault, not Google Authenticator.**
It synchronises the user's own machines and stores an encrypted backup. The vault's
first population comes from a single Google Authenticator export QR, done once. A
first-run wizard walks the user through it.

This distinction must be stated plainly in the README and in the wizard. A user who
expects sign-in to import their existing Google Authenticator accounts will otherwise
believe the application is broken.

---

## 3. Technology

Tauri 2 with a Rust core and a React 19 front end built by Vite, in TypeScript. This
mirrors `privum-cloud/remota`, which lets Tessera reuse its vault module and its
packaging approach, and keeps one stack for the team to maintain.

The Rust core holds every security-relevant operation: key derivation, encryption,
secret storage, code generation, and OAuth. The front end never sees a raw secret —
it receives generated codes and account metadata over Tauri commands.

---

## 4. Architecture

Rust modules under `src-tauri/src/`:

```
otp/          RFC 6238 TOTP and RFC 4226 HOTP. Pure functions, no I/O.
model/        Account and vault document types.
vault/        argon2id key derivation, AES-256-GCM sealing, file persistence.
import/       otpauth URIs, Google Authenticator protobuf, QR codec, rival formats.
screencap/    Screen QR capture through xdg-desktop-portal.
sync/         Google OAuth, Drive appDataFolder transport, merge.
commands.rs   The Tauri command surface exposed to the front end.
```

### 4.1 `otp/` — code generation

Implements RFC 6238 (TOTP) and RFC 4226 (HOTP), supporting HMAC-SHA1, HMAC-SHA256 and
HMAC-SHA512, six to eight digits, a configurable period, an HOTP counter, and the Steam
alphabet variant.

The module performs no I/O and holds no state. That isolation matters: it is the most
correctness-critical code in the application and also the easiest to prove correct. It
is validated against the official RFC 6238 test vectors, which publish expected codes
for a known seed at known timestamps across all three hash algorithms.

Crates: `hmac`, `sha1`, `sha2`, `base32`.

### 4.2 `model/` — data model

```rust
struct Account {
    id: Uuid,
    issuer: String,
    label: String,
    secret: Zeroizing<Vec<u8>>,
    kind: AccountKind,        // Totp | Hotp | Steam
    algorithm: Algorithm,     // Sha1 | Sha256 | Sha512
    digits: u8,               // 6..=8
    period: u32,              // seconds, TOTP only
    counter: u64,             // HOTP only
    icon: Option<String>,
    group: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    deleted_at: Option<DateTime<Utc>>,
}
```

Two decisions here are cheap now and expensive to retrofit:

**Carry every field from the start.** The Google Authenticator protobuf and the
`otpauth://` URI both express type, algorithm, digits, period, counter, issuer and
label. The model supports all of them even where the interface initially exposes only
the common case of TOTP/SHA1/6 digits/30 seconds. Discarding a field on import loses
data the user cannot recover.

**Soft delete is mandatory.** `deleted_at` is a tombstone. Without one, deleting an
account on machine A cannot propagate to machine B — the next merge sees an account
present on B and absent on A, and resurrects it. Tombstones are retained for 90 days,
then purged.

### 4.3 `vault/` — encryption at rest

Adapted from Remota's `src-tauri/src/vault/`, which already implements this pattern.

The master password is stretched with **argon2id** into a 256-bit key. The vault
document is serialised and sealed with **AES-256-GCM**; the authentication tag doubles
as the check that the password was correct. Keys are wrapped in `Zeroizing` and cleared
from memory on lock.

The vault lives at `~/.local/share/tessera/vault.bin`, following the XDG base directory
specification. Writes are atomic — write to a temporary file, then rename — so an
interrupted save cannot corrupt the vault.

The Google refresh token is stored **inside the encrypted vault**, never as a plaintext
file on disk. Synchronisation is consequently only possible while the vault is unlocked,
which is the correct constraint.

### 4.4 `import/` — getting accounts in and out

A desktop has no camera, so account entry is entirely a data-import problem. This
module is therefore larger than it looks.

- `otpauth.rs` — parse and emit `otpauth://totp/...` and `otpauth://hotp/...` URIs.
- `gauth.rs` — decode and encode `otpauth-migration://offline?data=` payloads. The
  Google Authenticator protobuf schema is declared locally and handled with `prost`.
  The encode direction is what sends accounts back to the phone.
- `qr.rs` — decode QR codes from images with `rqrr`; render QR codes with `qrcode` for
  the export screen.
- `aegis.rs`, `twofas.rs`, `andotp.rs`, `freeotp.rs` — read the backup files of the
  other open-source authenticators. Aegis and andOTP backups may themselves be
  password-encrypted; the importer prompts for that password and decrypts before
  parsing.

### 4.5 `screencap/` — reading a QR code off the screen

The everyday flow is enrolling a new account while a website displays its QR code.
Tessera captures the screen through `xdg-desktop-portal`'s Screenshot interface using
the `ashpd` crate, then decodes the image with `qr.rs`.

Going through the portal rather than an X11-specific API means this works under both
Wayland and X11, and asks the user's permission through the compositor's own dialog.

### 4.6 `sync/` — Google account synchronisation

**Authentication.** OAuth 2.0 authorization code flow with **PKCE**, redirecting to a
loopback address (`http://127.0.0.1:<ephemeral port>`). This is the flow Google
prescribes for installed applications. Scopes requested:

- `openid`, `email`, `profile` — to identify the account in the interface.
- `https://www.googleapis.com/auth/drive.appdata` — the hidden per-application folder.

`drive.appdata` is classified **Recommended / Non-sensitive** in Google's Drive API
scope table, requiring only basic app verification and no CASA security assessment.
This is what makes Google sign-in viable for a public open-source application, and it
was verified before this design was committed to.

Because Tessera is an installed application, it is a *public* OAuth client: the client
ID ships inside the binary, which Google's model expects and PKCE accounts for. The
practical constraint is not secrecy but verification — until the consent screen is
verified, Google shows an "unverified app" warning and caps the application at roughly
100 users.

**Transport.** `drive.rs` uses the Drive v3 files API confined to `appDataFolder`. It
uploads and downloads exactly one object: the **sealed vault blob**, byte-identical in
format to the local file. Google stores ciphertext it cannot read.

**Merge.** `merge.rs` reconciles two vault documents:

- Accounts are matched by `id` (a UUID assigned at creation, stable across devices).
- Where both sides hold the same `id`, the higher `updated_at` wins.
- A tombstone beats a live record of equal or older `updated_at`.
- Accounts present on only one side are kept.
- Each vault carries a `device_id` and a monotonic `revision`, so a device can tell
  whether the remote has changed since its last upload and skip a needless merge.

Synchronisation runs on unlock, on change, and on explicit request. A merge conflict
never destroys data: the losing version of a modified account is retained in a
`conflicts` list surfaced in the interface.

---

## 5. User interface

**Unlock.** Master password prompt on every launch. No option to skip it.

**Main view.** A searchable list of accounts. Each row shows issuer, label, the current
code, and a ring counting down to the next period boundary. Clicking a row copies the
code. Accounts can be filtered by group.

**Adding an account.** Four routes, matching section 4.4 and 4.5: paste an `otpauth://`
URI, enter the fields by hand, capture a QR code from the screen, or import a file
(a Google Authenticator export image, or a rival authenticator's backup).

**Account editor.** Edit issuer, label, icon and group. Delete — which writes a
tombstone rather than removing the record.

**Export.** Produce Google Authenticator migration QR codes for transfer back to the
phone, plain `otpauth://` URIs, or an encrypted backup file.

**Settings.** Auto-lock timeout, clipboard clear timeout, connected Google account and
its status, and a manual "Sync now".

**First-run wizard.** Sets the master password, connects the Google account, and then
walks the user through exporting from Google Authenticator and importing that QR —
the two minutes that populate the vault. The wizard states explicitly that signing in
with Google does not itself import Google Authenticator accounts.

---

## 6. Security model

**What Tessera protects against.** Someone who obtains the vault file — from a stolen
laptop, a backup, or the user's Google Drive — learns nothing without the master
password. Google itself is in this category: it holds only ciphertext.

**What it does not protect against.** An attacker executing code as the user while the
vault is unlocked can read secrets from memory. This is true of every authenticator and
is not solvable at this layer.

Decisions:

- **The master password is required at every launch**, and is not backed by the OS
  keyring. This was chosen deliberately over a libsecret-backed key: it keeps the
  derived key off disk entirely, at the cost of typing a password each session.
- **Auto-lock on inactivity**, default five minutes, configurable. Locking zeroizes the
  derived key and clears generated codes from the interface.
- **Clipboard auto-clear**, default twenty seconds. Behaviour varies between Wayland
  compositors — on some, clipboard contents do not outlive the owning application — so
  the implementation must verify what `tauri-plugin-clipboard-manager` actually does
  rather than assume, and the interface must not promise behaviour it cannot deliver.
- **No telemetry**, no analytics, no network traffic other than Google OAuth and Drive.

**A caveat the README must carry.** Holding TOTP secrets on the same machine as the
browser weakens the premise of a second factor: one compromised machine yields both
factors. This is a real trade-off users make for convenience, and Tessera should say
so rather than pretend otherwise.

---

## 7. Testing

- **`otp/`** — the RFC 6238 published test vectors, covering SHA1, SHA256 and SHA512 at
  known timestamps, plus RFC 4226's HOTP vectors. These are golden tests: any change
  that breaks them is a change that produces wrong codes.
- **`import/`** — round-trip tests for the Google Authenticator protobuf (decode an
  export, re-encode it, and get identical accounts), the `otpauth://` URI grammar
  including percent-encoded labels and issuers, and fixture files from each rival
  authenticator.
- **`vault/`** — seal and open round-trips, wrong-password rejection, atomic-write
  behaviour under an interrupted save.
- **`sync/merge.rs`** — the conflict matrix: concurrent add, concurrent edit, delete
  against edit, tombstone expiry, and a device rejoining after being offline.
- **End-to-end** — a full import from a real Google Authenticator export, then export
  back, verifying the codes agree with the phone.

Development follows the project's TDD practice: tests precede implementation.

---

## 8. Distribution

GitHub Actions builds on a `v*` tag and publishes the release itself: deb, rpm,
AppImage, and Flatpak.

This is **new work**. Remota has no `.github/` directory — its releases are built by
hand today. The existing GitHub Actions precedent in the organisation is a container
pipeline publishing to GHCR, which does not transfer to desktop bundles. Tessera's
workflow becomes the pattern, and Remota can adopt it afterwards.

The README follows the Remota landing-page pattern: screenshots, features, install
instructions, and a backlink to https://privum.cloud, with repository topics and
description set for discovery.

---

## 9. Implementation order

Every item ships in v1. The ordering exists so there is a working application early.

1. Scaffold — Tauri 2, React, Vite, TypeScript, and the CI skeleton.
2. `otp/` core, test-driven against the RFC vectors.
3. `vault/`, adapted from Remota.
4. Model, CRUD, and a usable interface — add, copy, edit, delete, search.
5. `import/` — otpauth URIs, Google Authenticator protobuf, QR codec, rival formats.
6. `screencap/` — portal integration.
7. `sync/` — Google OAuth, Drive transport, merge.
8. Packaging, CI, README landing page.

---

## 10. Dependencies outside the codebase

These block section 9 step 7 and need action from Marcio:

1. **Google Cloud project** with an OAuth client of type *Desktop app*.
2. **OAuth consent screen** configured with the `drive.appdata` scope, the application
   name, and a privacy policy URL on privum.cloud.
3. **Basic app verification** submitted, to remove the unverified-app warning and the
   ~100 user cap. No CASA assessment is required, because `drive.appdata` is
   non-sensitive.
4. **Trademark check on "Tessera"** before the name is committed to publicly.

Implementation of everything except step 7 can proceed while these are pending.
