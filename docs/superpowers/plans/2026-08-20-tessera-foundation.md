# Tessera Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Tessera application skeleton and a fully tested one-time-password engine, so that every later plan builds on code proven correct against the RFC test vectors.

**Architecture:** A Tauri 2 desktop application with a Rust core and a React front end. This plan delivers the core's innermost layer — `otp/`, which turns a secret and a moment in time into a code, and `model/`, which describes an account — plus the application shell that hosts them and the CI that guards them. `otp/` performs no I/O and holds no state, which is what makes it exhaustively testable.

**Tech Stack:** Rust 1.96, Tauri 2, React 19, Vite 7, TypeScript 5.8, Node 20. Crates: `hmac`, `sha1`, `sha2`, `base32`, `serde`, `uuid`, `chrono`, `zeroize`.

**Spec:** `docs/superpowers/specs/2026-08-20-tessera-design.md`

## Global Constraints

- **License:** GPL-3.0-only. `Cargo.toml` and `package.json` both declare it.
- **Language:** every user-facing string, comment, commit message, and document is in English.
- **Application identifier:** `cloud.privum.tessera`. **Product name:** `Tessera`.
- **The front end never receives a raw secret.** Tauri commands return generated codes and metadata only. This is a hard rule, not a preference.
- **Secrets are wrapped in `Zeroizing`** wherever they are held in memory.
- **TDD:** the failing test is written and observed failing before the implementation exists.
- **No telemetry**, no analytics, no network calls in this plan.
- **Vite dev port is 1431**, chosen so it collides with neither Tauri's 1420 default (which VS Code auto-forwards) nor Remota's 1430.

---

### Task 1: Application scaffold

Creates a Tauri 2 application that compiles, launches, and runs an empty Rust test suite. Nothing in this task is interesting on its own; everything afterwards depends on it existing.

The repository currently holds a `.gitignore` copied from a Dynamics 365 AL template, which ignores nothing relevant and ignores `.vscode/` for the wrong reason. It is replaced here.

**Files:**
- Create: `package.json`, `vite.config.ts`, `tsconfig.json`, `tsconfig.node.json`, `index.html`
- Create: `src/main.tsx`, `src/App.tsx`, `src/app.css`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Create: `src-tauri/capabilities/default.json`
- Create: `assets/logo.svg`, and generated `src-tauri/icons/*`
- Replace: `.gitignore`

**Interfaces:**
- Consumes: nothing.
- Produces: a crate named `tessera` with library target `tessera_lib` exposing `pub fn run()`; `npm run tauri dev` and `cargo test` both function.

- [ ] **Step 1: Replace the .gitignore**

```bash
cat > .gitignore <<'EOF'
# Rust
/src-tauri/target/
/src-tauri/gen/schemas/

# Node
node_modules/
dist/
*.tsbuildinfo

# Build artefacts
/packages/
*.deb
*.rpm
*.AppImage
*.flatpak

# Editors
.vscode/
.idea/
*.swp

# Local state — never commit a vault
*.bin
vault.bin
EOF
```

- [ ] **Step 2: Create the front-end manifest and configuration**

```bash
cat > package.json <<'EOF'
{
  "name": "tessera",
  "private": true,
  "version": "0.1.0",
  "license": "GPL-3.0-only",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "typecheck": "tsc --noEmit"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-clipboard-manager": "^2",
    "@tauri-apps/plugin-dialog": "^2",
    "@tauri-apps/plugin-opener": "^2",
    "react": "^19.1.0",
    "react-dom": "^19.1.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "@types/react": "^19.1.8",
    "@types/react-dom": "^19.1.6",
    "@vitejs/plugin-react": "^4.6.0",
    "typescript": "~5.8.3",
    "vite": "^7.0.4"
  }
}
EOF

cat > vite.config.ts <<'EOF'
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  // Let Rust compiler errors survive on screen instead of being cleared away.
  clearScreen: false,
  server: {
    // 1431 avoids both Tauri's 1420 default (VS Code auto-forwards it) and Remota's 1430.
    port: 1431,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1432 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
}));
EOF

cat > tsconfig.json <<'EOF'
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
EOF

cat > tsconfig.node.json <<'EOF'
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true
  },
  "include": ["vite.config.ts"]
}
EOF

cat > index.html <<'EOF'
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Tessera</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
EOF
```

- [ ] **Step 3: Create the minimal React entry point**

```bash
mkdir -p src
cat > src/main.tsx <<'EOF'
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./app.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
EOF

cat > src/App.tsx <<'EOF'
export default function App() {
  return <main className="shell">Tessera</main>;
}
EOF

cat > src/app.css <<'EOF'
:root {
  color-scheme: dark;
  font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
}

body {
  margin: 0;
  background: #0e1117;
  color: #e6e9ef;
}

.shell {
  display: grid;
  place-items: center;
  min-height: 100vh;
}
EOF
```

- [ ] **Step 4: Create the Rust crate**

```bash
mkdir -p src-tauri/src src-tauri/capabilities
cat > src-tauri/Cargo.toml <<'EOF'
[package]
name = "tessera"
version = "0.1.0"
description = "Tessera — open-source authenticator for Linux"
authors = ["Privum Cloud"]
license = "GPL-3.0-only"
edition = "2021"

[lib]
# The `_lib` suffix keeps the library name distinct from the binary name.
name = "tessera_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
tauri-plugin-dialog = "2"
tauri-plugin-clipboard-manager = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
zeroize = { version = "1", features = ["zeroize_derive"] }
hmac = "0.12"
sha1 = "0.10"
sha2 = "0.10"
base32 = "0.5"
thiserror = "2"
EOF

cat > src-tauri/build.rs <<'EOF'
fn main() {
    tauri_build::build()
}
EOF

cat > src-tauri/src/main.rs <<'EOF'
// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tessera_lib::run()
}
EOF

cat > src-tauri/src/lib.rs <<'EOF'
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .run(tauri::generate_context!())
        .expect("error while running Tessera");
}
EOF

cat > src-tauri/capabilities/default.json <<'EOF'
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "opener:default",
    "dialog:default",
    "clipboard-manager:allow-read-text",
    "clipboard-manager:allow-write-text"
  ]
}
EOF

cat > src-tauri/tauri.conf.json <<'EOF'
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Tessera",
  "version": "0.1.0",
  "identifier": "cloud.privum.tessera",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1431",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "Tessera",
        "width": 460,
        "height": 720,
        "minWidth": 380,
        "minHeight": 560,
        "dragDropEnabled": false
      }
    ],
    "security": {
      "csp": "default-src 'self'; img-src 'self' data: asset: http://asset.localhost; style-src 'self' 'unsafe-inline'"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "category": "Utility",
    "shortDescription": "Open-source authenticator for Linux",
    "longDescription": "Tessera generates TOTP and HOTP codes, imports accounts from Google Authenticator, and synchronises an encrypted vault through your Google account.",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "linux": {
      "deb": {
        "depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0"]
      }
    }
  }
}
EOF
```

Note the window is 460×720 — an authenticator is a tall narrow list, not a 1100px workbench like Remota.

Note also the `csp` is set rather than left `null`. Remota sets it to `null` because it proxies remote desktop streams; Tessera has no such need, and a real policy costs nothing here.

- [ ] **Step 5: Create the placeholder icon and generate the icon set**

A real logo arrives in the packaging plan. This produces a legible placeholder so the bundle has something to ship and the window has something to show.

```bash
mkdir -p assets
cat > assets/logo.svg <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="1024" height="1024">
  <rect width="512" height="512" rx="112" fill="#12161f"/>
  <circle cx="256" cy="256" r="150" fill="none" stroke="#3d7dff" stroke-width="28"/>
  <path d="M256 106 A150 150 0 0 1 406 256" fill="none" stroke="#5ce1a6"
        stroke-width="28" stroke-linecap="round"/>
  <rect x="212" y="216" width="88" height="26" rx="13" fill="#e6e9ef"/>
  <rect x="212" y="264" width="60" height="26" rx="13" fill="#8b93a7"/>
</svg>
EOF

rsvg-convert -w 1024 -h 1024 assets/logo.svg -o assets/logo.png
npm install
npm run tauri icon assets/logo.png
```

- [ ] **Step 6: Verify the crate compiles and the empty test suite runs**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: compiles, then `test result: ok. 0 passed`.

Run: `npm run typecheck`
Expected: no output, exit 0.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: scaffold the Tauri application

Replaces the Dynamics 365 AL .gitignore the repository was created with.
Window is sized for a tall account list rather than a workbench, and the
content security policy is a real policy rather than null."
```

---

### Task 2: OTP foundation — algorithm, truncation, HOTP, and secrets

The innermost layer. Every code Tessera ever shows passes through `truncate`, so this is the code that most deserves the RFC's own test vectors rather than tests we invent.

The test vectors below are from RFC 4226 Appendix D, using the specification's secret `12345678901234567890`. **This code has been verified to pass before being written into this plan.**

**Files:**
- Create: `src-tauri/src/otp/mod.rs`
- Create: `src-tauri/src/otp/algorithm.rs`
- Create: `src-tauri/src/otp/secret.rs`
- Modify: `src-tauri/src/lib.rs` (declare `mod otp;`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `pub enum Algorithm { Sha1, Sha256, Sha512 }` — `Copy`, `Clone`, `Debug`, `PartialEq`, `Eq`, `Serialize`, `Deserialize`, `Default` (SHA-1).
  - `pub fn hotp(alg: Algorithm, key: &[u8], counter: u64, digits: u32) -> String`
  - `pub struct Secret(Zeroizing<Vec<u8>>)` with `Secret::from_base32(&str) -> Result<Secret, OtpError>`, `Secret::from_bytes(Vec<u8>) -> Secret`, `fn expose(&self) -> &[u8]`, and `fn to_base32(&self) -> Zeroizing<String>`.
  - `pub enum OtpError` with variant `InvalidSecret`.

- [ ] **Step 1: Write the failing tests**

```bash
mkdir -p src-tauri/src/otp
cat > src-tauri/src/otp/algorithm.rs <<'EOF'
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

#[cfg(test)]
mod tests {
    use super::super::hotp;
    use super::Algorithm;

    /// RFC 4226 Appendix D uses this ASCII secret throughout.
    const SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn rfc4226_appendix_d_vectors() {
        let expected = [
            "755224", "287082", "359152", "969429", "338314",
            "254676", "287922", "162583", "399871", "520489",
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
}
EOF
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test otp:: 2>&1 | tail -20`
Expected: FAIL — the compiler cannot resolve `super::super::hotp`, and `mod otp` is not declared.

- [ ] **Step 3: Write the implementation**

Append the implementation to `algorithm.rs`, above the `#[cfg(test)]` block:

```rust
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
            let mut m = <HmacSha1 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
            m.update(message);
            m.finalize().into_bytes().to_vec()
        }
        Algorithm::Sha256 => {
            let mut m = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
            m.update(message);
            m.finalize().into_bytes().to_vec()
        }
        Algorithm::Sha512 => {
            let mut m = <HmacSha512 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
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
```

Now create the module root:

```bash
cat > src-tauri/src/otp/mod.rs <<'EOF'
//! One-time-password generation.
//!
//! This module performs no I/O and holds no state: it turns a secret and a
//! counter into a string of digits. That isolation is deliberate — it is the
//! most correctness-critical code in Tessera and also the easiest to test
//! exhaustively, because the RFCs publish their own expected outputs.

mod algorithm;

pub use algorithm::Algorithm;

use algorithm::{mac, truncate};

/// RFC 4226 counter-based one-time password.
pub fn hotp(alg: Algorithm, key: &[u8], counter: u64, digits: u32) -> String {
    let binary = truncate(&mac(alg, key, &counter.to_be_bytes()));
    // u64 and capped, because `digits` is not ours to trust: it is deserialised
    // from the vault document and, once importing exists, from a Google
    // Authenticator protobuf we did not write. 10^10 overflows a u32 and 10^20
    // overflows a u64, and an overflow here is a panic in debug builds.
    let modulus = 10u64.pow(digits.min(19));
    format!("{:0width$}", u64::from(binary) % modulus, width = digits as usize)
}
EOF
```

And declare the module in `src-tauri/src/lib.rs` by inserting `pub mod otp;` as the first line of the file. It is public because it is the library's real surface; private would make test-only items read as dead code.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test otp:: 2>&1 | tail -10`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 5: Write the failing secret tests**

```bash
cat > src-tauri/src/otp/secret.rs <<'EOF'
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
        assert_eq!(Secret::from_base32("not base32!"), Err(OtpError::InvalidSecret));
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
        assert!(!rendered.contains("12345"), "Debug leaked the secret: {rendered}");
    }
}
EOF
```

- [ ] **Step 6: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test otp::secret 2>&1 | tail -20`
Expected: FAIL — `Secret::from_base32`, `from_bytes`, `expose`, and `to_base32` do not exist.

- [ ] **Step 7: Write the implementation**

Insert into `secret.rs`, between the `pub struct Secret` declaration and the `#[cfg(test)]` block:

```rust
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
        Zeroizing::new(base32::encode(Alphabet::Rfc4648 { padding: false }, &self.0))
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
```

Now declare the module. `otp/mod.rs` could not name `secret` until this point, because
the file did not exist. Add beneath `mod algorithm;`:

```rust
mod secret;
```

and beneath `pub use algorithm::Algorithm;`:

```rust
pub use secret::{OtpError, Secret};
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test otp:: 2>&1 | tail -10`
Expected: `test result: ok. 8 passed`.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(otp): add HMAC dispatch, dynamic truncation, HOTP, and secrets

HOTP is verified against the RFC 4226 Appendix D vectors. Secret holds its
bytes in Zeroizing and implements Debug by hand so a stray format string
cannot print a shared secret."
```

---

### Task 3: TOTP

TOTP is HOTP with the counter derived from the clock. The value of this task is not the arithmetic — it is the RFC 6238 vectors, which pin all three hash algorithms at six timestamps, including one past the 32-bit epoch boundary where a narrower integer type would break.

**Files:**
- Create: `src-tauri/src/otp/totp.rs`
- Modify: `src-tauri/src/otp/mod.rs` (declare and re-export)

**Interfaces:**
- Consumes: `Algorithm` and `hotp` from Task 2.
- Produces:
  - `pub fn totp_at(alg: Algorithm, key: &[u8], unix_seconds: u64, period: u32, digits: u32) -> String`
  - `pub fn seconds_remaining(unix_seconds: u64, period: u32) -> u32`

- [ ] **Step 1: Write the failing tests**

```bash
cat > src-tauri/src/otp/totp.rs <<'EOF'
//! RFC 6238 time-based one-time passwords.

use super::{hotp, Algorithm};

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 Appendix B. The secret is the RFC 4226 secret repeated to reach
    // each hash's block size, which is why the three differ in length.
    const SHA1_KEY: &[u8] = b"12345678901234567890";
    const SHA256_KEY: &[u8] = b"12345678901234567890123456789012";
    const SHA512_KEY: &[u8] =
        b"1234567890123456789012345678901234567890123456789012345678901234";

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
            assert_eq!(&totp_at(Algorithm::Sha1, SHA1_KEY, *time, 30, 8), sha1,
                       "SHA-1 diverged at t={time}");
            assert_eq!(&totp_at(Algorithm::Sha256, SHA256_KEY, *time, 30, 8), sha256,
                       "SHA-256 diverged at t={time}");
            assert_eq!(&totp_at(Algorithm::Sha512, SHA512_KEY, *time, 30, 8), sha512,
                       "SHA-512 diverged at t={time}");
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
EOF
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test otp::totp 2>&1 | tail -20`
Expected: FAIL — `totp_at` and `seconds_remaining` are not defined, and `mod totp` is not declared.

- [ ] **Step 3: Write the implementation**

Insert into `totp.rs`, above the `#[cfg(test)]` block:

```rust
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
```

Then declare it in `src-tauri/src/otp/mod.rs`: add `mod totp;` beneath `mod secret;`, and `pub use totp::{seconds_remaining, totp_at};` beneath the existing re-exports.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test otp:: 2>&1 | tail -10`
Expected: `test result: ok. 12 passed`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(otp): add TOTP verified against the RFC 6238 vectors

Covers SHA-1, SHA-256 and SHA-512 at six timestamps including one past the
32-bit epoch boundary, which is where a narrower counter type would fail."
```

---

### Task 4: The Steam variant

Steam issues five-character alphabetic codes rather than digits. It is the one non-standard variant common enough to be worth supporting, and users who hold one will otherwise conclude Tessera is broken.

**Files:**
- Create: `src-tauri/src/otp/steam.rs`
- Modify: `src-tauri/src/otp/mod.rs`

**Interfaces:**
- Consumes: `Algorithm`, `mac` and `truncate` from Task 2.
- Produces: `pub fn steam_at(key: &[u8], unix_seconds: u64) -> String`

- [ ] **Step 1: Write the failing tests**

```bash
cat > src-tauri/src/otp/steam.rs <<'EOF'
//! Steam's five-character variant of TOTP.
//!
//! Steam uses the standard HMAC-SHA1 dynamic truncation, then renders the
//! result in base 26 over its own alphabet instead of base 10. The alphabet
//! omits characters that are easy to misread aloud or on screen.

use super::algorithm::{mac, truncate};
use super::Algorithm;

const ALPHABET: &[u8] = b"23456789BCDFGHJKMNPQRTVWXY";
const CODE_LENGTH: usize = 5;

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
EOF
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test otp::steam 2>&1 | tail -20`
Expected: FAIL — `steam_at` is not defined and `mod steam` is not declared.

- [ ] **Step 3: Write the implementation**

Insert into `steam.rs`, above the `#[cfg(test)]` block:

```rust
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
```

`mac` and `truncate` are `pub(crate)`, so `steam.rs` reaches them through `super::algorithm`. Add `mod steam;` and `pub use steam::steam_at;` to `src-tauri/src/otp/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test otp:: 2>&1 | tail -10`
Expected: `test result: ok. 15 passed`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(otp): add the Steam five-character code variant"
```

---

### Task 5: The account model

The type every later plan reads and writes. Two of its decisions exist purely to serve synchronisation, which is four plans away — they are made now because retrofitting either one would mean migrating vaults that already exist.

**Files:**
- Create: `src-tauri/src/model/mod.rs`
- Create: `src-tauri/src/model/account.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `Algorithm` and `Secret` from Task 2.
- Produces:
  - `pub enum AccountKind { Totp, Hotp, Steam }`
  - `pub struct Account` with fields `id: Uuid`, `issuer: String`, `label: String`, `secret: Secret`, `kind: AccountKind`, `algorithm: Algorithm`, `digits: u32`, `period: u32`, `counter: u64`, `icon: Option<String>`, `group: Option<String>`, `created_at: DateTime<Utc>`, `updated_at: DateTime<Utc>`, `deleted_at: Option<DateTime<Utc>>`, `revision: u64`
  - `Account::new(issuer, label, secret) -> Account`
  - `fn touch(&mut self)` — bumps `revision` and `updated_at`
  - `fn soft_delete(&mut self)` — sets `deleted_at` and touches
  - `fn is_deleted(&self) -> bool`
  - `fn display_name(&self) -> String`

- [ ] **Step 1: Write the failing tests**

```bash
mkdir -p src-tauri/src/model
cat > src-tauri/src/model/account.rs <<'EOF'
//! An account: the thing a code is generated for.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::otp::{Algorithm, Secret};

/// What kind of one-time password this account issues.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    #[default]
    Totp,
    Hotp,
    Steam,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Account {
        Account::new(
            "GitHub".into(),
            "marcio@privum.cloud".into(),
            Secret::from_bytes(b"12345678901234567890".to_vec()),
        )
    }

    #[test]
    fn a_new_account_defaults_to_the_shape_services_actually_issue() {
        let account = sample();
        assert_eq!(account.kind, AccountKind::Totp);
        assert_eq!(account.algorithm, Algorithm::Sha1);
        assert_eq!(account.digits, 6);
        assert_eq!(account.period, 30);
        assert_eq!(account.counter, 0);
        assert_eq!(account.revision, 1);
        assert!(!account.is_deleted());
    }

    #[test]
    fn touch_advances_the_revision() {
        // revision, not updated_at, is what decides a merge — a machine with a
        // skewed clock must not be able to win one.
        let mut account = sample();
        let before = account.revision;
        account.touch();
        assert_eq!(account.revision, before + 1);
    }

    #[test]
    fn soft_delete_leaves_a_tombstone_rather_than_removing_the_record() {
        // Without a tombstone a delete cannot propagate: the next sync would see
        // the account present on the other machine and resurrect it.
        let mut account = sample();
        let before = account.revision;
        account.soft_delete();
        assert!(account.is_deleted());
        assert!(account.deleted_at.is_some());
        assert!(account.revision > before, "a delete must advance the revision");
    }

    #[test]
    fn display_name_joins_issuer_and_label_but_copes_without_an_issuer() {
        assert_eq!(sample().display_name(), "GitHub (marcio@privum.cloud)");

        let mut anonymous = sample();
        anonymous.issuer = String::new();
        assert_eq!(anonymous.display_name(), "marcio@privum.cloud");
    }

    #[test]
    fn survives_a_serde_round_trip_with_every_field_intact() {
        // Every field of the Google Authenticator protobuf and the otpauth URI
        // is carried, even where the interface does not yet expose it. Dropping
        // one on import would lose data the user cannot recover.
        let mut original = sample();
        original.kind = AccountKind::Hotp;
        original.algorithm = Algorithm::Sha512;
        original.digits = 8;
        original.period = 60;
        original.counter = 42;
        original.icon = Some("github".into());
        original.group = Some("Work".into());

        let json = serde_json::to_string(&original).unwrap();
        let restored: Account = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.id, original.id);
        assert_eq!(restored.kind, original.kind);
        assert_eq!(restored.algorithm, original.algorithm);
        assert_eq!(restored.digits, original.digits);
        assert_eq!(restored.period, original.period);
        assert_eq!(restored.counter, original.counter);
        assert_eq!(restored.icon, original.icon);
        assert_eq!(restored.group, original.group);
        assert_eq!(restored.secret, original.secret);
        assert_eq!(restored.revision, original.revision);
    }
}
EOF
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test model:: 2>&1 | tail -20`
Expected: FAIL — `Account` is not defined and `mod model` is not declared.

- [ ] **Step 3: Add serde support to Secret**

`Account` derives `Serialize`/`Deserialize`, so `Secret` needs both. Serialising as base32 keeps the vault document readable when debugging with the vault unlocked, and matches how the secret is written everywhere else.

Append to `src-tauri/src/otp/secret.rs`, after the `impl Eq for Secret` block:

```rust
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
```

No extra import is needed: `String::deserialize` resolves without bringing the trait into scope.

- [ ] **Step 4: Write the Account implementation**

Insert into `account.rs`, above the `#[cfg(test)]` block:

```rust
/// An account Tessera generates codes for.
///
/// Two fields exist for synchronisation rather than for display:
///
/// `revision` is incremented on every local edit and is the authority when two
/// machines disagree. A wall clock is not: a laptop with a skewed clock would
/// otherwise win every merge, permanently and invisibly.
///
/// `deleted_at` is a tombstone. Removing the record outright would make the
/// deletion unpropagatable — the next merge would see the account alive on the
/// other machine and bring it back.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub issuer: String,
    pub label: String,
    pub secret: Secret,
    pub kind: AccountKind,
    pub algorithm: Algorithm,
    pub digits: u32,
    pub period: u32,
    /// HOTP only. The one mutable field, which is why merging takes its maximum
    /// rather than the most recent value — lost increments lock the user out.
    pub counter: u64,
    pub icon: Option<String>,
    pub group: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub revision: u64,
}

impl Account {
    /// A new TOTP account with the defaults nearly every service issues:
    /// HMAC-SHA1, six digits, thirty seconds.
    pub fn new(issuer: String, label: String, secret: Secret) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            issuer,
            label,
            secret,
            kind: AccountKind::Totp,
            algorithm: Algorithm::Sha1,
            digits: 6,
            period: 30,
            counter: 0,
            icon: None,
            group: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            revision: 1,
        }
    }

    /// Record a local edit. Every mutation must go through this.
    pub fn touch(&mut self) {
        self.revision += 1;
        self.updated_at = Utc::now();
    }

    /// Mark the account deleted without discarding it, so the deletion can
    /// reach the user's other machines.
    pub fn soft_delete(&mut self) {
        self.deleted_at = Some(Utc::now());
        self.touch();
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// How the account reads in the list.
    pub fn display_name(&self) -> String {
        if self.issuer.is_empty() {
            self.label.clone()
        } else {
            format!("{} ({})", self.issuer, self.label)
        }
    }
}
```

Create the module root:

```bash
cat > src-tauri/src/model/mod.rs <<'EOF'
//! The types Tessera stores.

mod account;

pub use account::{Account, AccountKind};
EOF
```

Add `pub mod model;` to `src-tauri/src/lib.rs` beneath `pub mod otp;`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: `test result: ok. 20 passed`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(model): add the Account type with revision and tombstone

revision rather than updated_at is the merge authority, so a machine with a
skewed clock cannot win every merge; deleted_at is a tombstone, without which
a deletion cannot propagate and the next sync resurrects the account."
```

---

### Task 6: The command surface and an end-to-end smoke screen

Proves the whole stack in one line of glue: a secret entered in the browser layer reaches Rust, becomes a code, and comes back — without the secret ever travelling in the other direction.

The screen built here is deliberately plain. The real interface is designed in the next plan, with the frontend-design skill; this is the wire, not the fitting.

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/App.tsx`
- Create: `src/lib/api.ts`

**Interfaces:**
- Consumes: `Account` from Task 5; `totp_at`, `steam_at`, `seconds_remaining`, `Secret` from Tasks 2–4.
- Produces:
  - Rust: `#[tauri::command] fn preview_code(secret: String, kind: AccountKind, algorithm: Algorithm, digits: u32, period: u32, counter: u64) -> Result<CodeView, String>`
  - Rust: `pub struct CodeView { pub code: String, pub seconds_remaining: u32 }`
  - TypeScript: `previewCode(input: PreviewInput): Promise<CodeView>` from `src/lib/api.ts`

- [ ] **Step 1: Write the failing test**

```bash
cat > src-tauri/src/commands.rs <<'EOF'
//! The Tauri command surface.
//!
//! Commands return generated codes and metadata. A raw secret never travels in
//! this direction — the front end has no need of one and no way to hold it
//! safely.

use serde::Serialize;

use crate::model::AccountKind;
use crate::otp::{seconds_remaining, steam_at, totp_at, Algorithm, Secret};

/// What the interface needs in order to render one row.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CodeView {
    pub code: String,
    pub seconds_remaining: u32,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the system clock is set before 1970")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_BASE32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn generates_a_totp_code_matching_the_rfc_vector() {
        let view = code_at(RFC_BASE32, AccountKind::Totp, Algorithm::Sha1, 8, 30, 0, 59).unwrap();
        assert_eq!(view.code, "94287082");
        assert_eq!(view.seconds_remaining, 1);
    }

    #[test]
    fn generates_an_hotp_code_from_the_counter_not_the_clock() {
        let view = code_at(RFC_BASE32, AccountKind::Hotp, Algorithm::Sha1, 6, 30, 1, 59).unwrap();
        assert_eq!(view.code, "287082");
        // An HOTP code does not expire, so there is nothing to count down.
        assert_eq!(view.seconds_remaining, 0);
    }

    #[test]
    fn generates_a_steam_code() {
        let view = code_at(RFC_BASE32, AccountKind::Steam, Algorithm::Sha1, 5, 30, 0, 59).unwrap();
        assert_eq!(view.code.len(), 5);
    }

    #[test]
    fn reports_a_bad_secret_as_an_error_rather_than_panicking() {
        let result = code_at("not base32!", AccountKind::Totp, Algorithm::Sha1, 6, 30, 0, 59);
        assert!(result.is_err());
    }
}
EOF
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test commands:: 2>&1 | tail -20`
Expected: FAIL — `code_at` is not defined and `mod commands` is not declared.

- [ ] **Step 3: Write the implementation**

Insert into `commands.rs`, above the `#[cfg(test)]` block:

```rust
/// Generate a code at an explicit moment.
///
/// Split out from the command so the clock can be supplied in tests; the
/// command itself is the same call with the real time.
fn code_at(
    secret: &str,
    kind: AccountKind,
    algorithm: Algorithm,
    digits: u32,
    period: u32,
    counter: u64,
    unix_seconds: u64,
) -> Result<CodeView, String> {
    let secret = Secret::from_base32(secret).map_err(|e| e.to_string())?;

    let (code, remaining) = match kind {
        AccountKind::Totp => (
            totp_at(algorithm, secret.expose(), unix_seconds, period, digits),
            seconds_remaining(unix_seconds, period),
        ),
        // An HOTP code stands until the user asks for the next one, so there is
        // no countdown to report.
        AccountKind::Hotp => (
            crate::otp::hotp(algorithm, secret.expose(), counter, digits),
            0,
        ),
        AccountKind::Steam => (
            steam_at(secret.expose(), unix_seconds),
            seconds_remaining(unix_seconds, 30),
        ),
    };

    Ok(CodeView { code, seconds_remaining: remaining })
}

/// Generate the code for an account as it stands right now.
#[tauri::command]
pub fn preview_code(
    secret: String,
    kind: AccountKind,
    algorithm: Algorithm,
    digits: u32,
    period: u32,
    counter: u64,
) -> Result<CodeView, String> {
    code_at(&secret, kind, algorithm, digits, period, counter, now_unix())
}
```

`hotp` must be reachable as `crate::otp::hotp` — confirm `pub use` in `otp/mod.rs` exports it (Task 2 declared it `pub fn` in the module root, so it already is).

Wire it up in `src-tauri/src/lib.rs`:

```rust
mod commands;
// `otp` and `model` are public because they are the library's real surface.
// Leaving them private makes every item used only by tests read as dead code,
// which would have to be silenced with allow(dead_code) — and that would hide
// genuinely dead code later.
pub mod model;
pub mod otp;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![commands::preview_code])
        .run(tauri::generate_context!())
        .expect("error while running Tessera");
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test 2>&1 | tail -10`
Expected: `test result: ok. 24 passed`.

- [ ] **Step 5: Write the TypeScript binding and the smoke screen**

```bash
mkdir -p src/lib
cat > src/lib/api.ts <<'EOF'
import { invoke } from "@tauri-apps/api/core";

export type AccountKind = "totp" | "hotp" | "steam";
export type Algorithm = "SHA1" | "SHA256" | "SHA512";

// A type alias rather than an interface: Tauri's `invoke` takes
// `Record<string, unknown>`, and TypeScript grants an implicit index signature
// to type aliases but not to interfaces.
export type PreviewInput = {
  secret: string;
  kind: AccountKind;
  algorithm: Algorithm;
  digits: number;
  period: number;
  counter: number;
};

export interface CodeView {
  code: string;
  secondsRemaining: number;
}

interface RawCodeView {
  code: string;
  seconds_remaining: number;
}

/** Generate the code for the given parameters as of now. */
export async function previewCode(input: PreviewInput): Promise<CodeView> {
  const raw = await invoke<RawCodeView>("preview_code", input);
  return { code: raw.code, secondsRemaining: raw.seconds_remaining };
}
EOF

cat > src/App.tsx <<'EOF'
import { useEffect, useState } from "react";
import { previewCode, type CodeView } from "./lib/api";

/** The RFC 4226 test secret, so the smoke screen has something to show. */
const SAMPLE_SECRET = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

export default function App() {
  const [view, setView] = useState<CodeView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const tick = () =>
      previewCode({
        secret: SAMPLE_SECRET,
        kind: "totp",
        algorithm: "SHA1",
        digits: 6,
        period: 30,
        counter: 0,
      })
        .then(setView)
        .catch((e) => setError(String(e)));

    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, []);

  return (
    <main className="shell">
      {error ? (
        <p role="alert">{error}</p>
      ) : (
        <div>
          <p className="code">{view?.code ?? "······"}</p>
          <p className="countdown">{view?.secondsRemaining ?? 0}s</p>
        </div>
      )}
    </main>
  );
}
EOF

cat >> src/app.css <<'EOF'

.code {
  font-size: 2.5rem;
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.12em;
  margin: 0;
}

.countdown {
  color: #8b93a7;
  text-align: center;
  margin: 0.25rem 0 0;
}
EOF
```

- [ ] **Step 6: Verify the front end type-checks and the app builds**

Run: `npm run typecheck`
Expected: exit 0, no output.

Run: `npm run tauri build -- --bundles deb 2>&1 | tail -5`
Expected: a `.deb` under `src-tauri/target/release/bundle/deb/`.

If a graphical session is available, run `npm run tauri dev` and confirm a six-digit code appears and the countdown decrements. This shell runs under `XDG_SESSION_TYPE=tty`, so that check may need to happen from the desktop session.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: add the command surface and an end-to-end smoke screen

A secret entered above the boundary becomes a code below it and comes back;
no raw secret travels the other way, which is the rule the command surface
exists to enforce."
```

---

### Task 7: Continuous integration

Guards the RFC vectors. Everything after this plan builds on `otp/`, so a regression there is the one failure that must never reach a tag.

This is new work for the organisation: Remota has no `.github/` directory and its releases are built by hand. The only existing GitHub Actions precedent publishes containers to GHCR, which does not transfer to a desktop application. Release automation arrives in the packaging plan; this task covers tests only.

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: the test suite from Tasks 2–6.
- Produces: a `ci` check on every push and pull request.

- [ ] **Step 1: Write the workflow**

```bash
mkdir -p .github/workflows
cat > .github/workflows/ci.yml <<'EOF'
name: ci

on:
  push:
    # feature branches too, so a broken workflow surfaces on the branch that
    # broke it rather than at merge time
    branches: [main, "feat/**"]
  pull_request:

jobs:
  rust:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4

      # Tauri links against the system webkit; without these the crate does not
      # build, even though nothing under test touches a window.
      - name: Install Tauri system dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
            libayatana-appindicator3-dev patchelf

      # Pinned to match rust-toolchain.toml. @stable drifts, and a lint added
      # to a newer clippy fails commits that were clean when written.
      - uses: dtolnay/rust-toolchain@1.96.0
        with:
          components: rustfmt, clippy

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri

      - name: Check formatting
        run: cargo fmt --all --check
        working-directory: src-tauri

      - name: Lint
        run: cargo clippy --all-targets -- -D warnings
        working-directory: src-tauri

      - name: Test
        run: cargo test --all
        working-directory: src-tauri

  web:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
      - run: npm ci
      - run: npm run typecheck
      - run: npm run build
EOF
```

- [ ] **Step 2: Verify the same commands pass locally before pushing**

Run: `cd src-tauri && cargo fmt --all --check`
Expected: exit 0. If it reports diffs, run `cargo fmt --all` and re-run.

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings 2>&1 | tail -20`
Expected: exit 0. Fix any lint rather than allowing it.

Run: `npm run build`
Expected: a `dist/` directory.

- [ ] **Step 3: Commit and push**

```bash
git add -A
git commit -m "ci: run fmt, clippy, tests and typecheck on push and pull request

Guards the RFC 4226 and 6238 vectors, which everything later in the project
builds on. Remota has no CI; this workflow becomes the pattern for both."
git push origin main
```

- [ ] **Step 4: Confirm the workflow passes**

Run: `gh run watch` (or `gh run list --limit 1`)
Expected: both jobs green. If `npm ci` fails for want of a lockfile, commit `package-lock.json` — `npm install` in Task 1 generated it.

---

## Definition of done

- `cargo test` passes 24 tests, including every RFC 4226 and RFC 6238 vector.
- `cargo fmt --check` and `cargo clippy -D warnings` are clean.
- `npm run typecheck` and `npm run build` succeed.
- `npm run tauri build --bundles deb` produces an installable package.
- CI is green on `main`.
- No raw secret is returned by any Tauri command.

## What this plan deliberately does not do

Encryption at rest, the account list, adding accounts, importing from anything, and synchronisation. Those are Plans 2 through 5. This plan exists so that when they arrive, the code underneath them is already known to be correct.
