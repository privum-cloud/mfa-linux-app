# macOS support — feasibility analysis

Date: 2026-08-29 · Branch: `analysis/macos-support` · Verified on macOS (Darwin 25.6, Apple Silicon, Rust 1.96.0, Node 22)

## Verdict

**Low effort.** The code needs one behavioural change; everything else is release
pipeline, signing and documentation. Nothing platform-specific in the Rust or
TypeScript sources blocks macOS.

Evidence gathered on this branch with **zero source changes**:

| Check | Result |
|---|---|
| `cargo test --all` | 153 passed, 0 failed |
| `npm run tauri build -- --bundles app,dmg` | `Tessera.app`, `Tessera_0.4.0_aarch64.dmg` and the updater artefact `Tessera.app.tar.gz` produced |
| Launch `Tessera.app` | Process starts, window opens, config written to `~/Library/Application Support/tessera/config.json` |

The only build error was the missing `TAURI_SIGNING_PRIVATE_KEY`, which CI already has as a secret.

## Why it already works

- `Cargo.toml` already gates `tauri-plugin-updater` on `linux | windows | macos`.
- `src-tauri/icons/icon.icns` already exists and is listed in `tauri.conf.json`.
- Every `#[cfg(unix)]` path (0o600 vault file, 0o700 directory, atomic same-directory rename in `vault/file.rs`) applies to macOS as-is.
- `dirs::data_dir()` / `dirs::config_dir()` resolve to `~/Library/Application Support/` on macOS — a sane, non-roaming location, consistent with the Windows `%LOCALAPPDATA%` reasoning in `vault/manager.rs`.
- Tauri on macOS uses the system WKWebView: no extra runtime to install (unlike WebView2 on Windows or webkit2gtk on Linux).
- The frontend has no Ctrl-vs-Cmd shortcuts to adapt.

## Work required

### 1. Code (small)

- `src-tauri/src/update.rs` — `delivery()` returns `NeedsAdmin` on any non-Windows, non-AppImage platform. On macOS the updater replaces the `.app` bundle in place with the user's own permissions, so this must return `SelfInstall` for `cfg!(target_os = "macos")`. Add a test alongside the existing ones.
- `src-tauri/tauri.conf.json` — `shortDescription` still says "for Linux"; add a `bundle.macOS` block (`minimumSystemVersion`, and later the signing settings). Same string in `Cargo.toml` `description`.

### 2. Release pipeline (`.github/workflows/release.yml`)

- Add a `macos` job (`runs-on: macos-latest`, Apple Silicon). Decision to make:
  - **Universal binary** (`--target universal-apple-darwin`, needs `rustup target add x86_64-apple-darwin aarch64-apple-darwin`) — one `.dmg`, one updater artefact, covers Intel and Apple Silicon. Recommended.
  - Or two jobs/targets producing `Tessera_x.y.z_aarch64.dmg` and `Tessera_x.y.z_x64.dmg`. Note the updater artefact is named `Tessera.app.tar.gz` **without an architecture suffix**, so two arch builds would overwrite each other on the release unless renamed.
- Upload `bundle/dmg/*.dmg` and `bundle/macos/*.app.tar.gz` + `.sig`.
- `publish` job: `needs: [linux, windows, macos]`.

### 3. Update manifest (`scripts/build-update-manifest.py`)

Add to `TARGETS`: `(".app.tar.gz.sig", ...)`. The updater plugin looks up `darwin-aarch64` / `darwin-x86_64` (from the running process's arch). With a universal build, both keys point at the same URL — the script needs to emit the entry twice, or accept a list of keys per suffix.

### 4. Code signing and notarization — optional, a UX decision

**An Apple Developer account is not required.** Tauri ad-hoc signs the bundle
(`codesign -s -`), which is all macOS needs to run it; the `.app` built on this
branch launched with no account and no certificate.

What the paid account (US$99/year, Developer ID + notarization) buys is only the
first-launch experience. Without it, a `.dmg` downloaded through a browser carries
the `com.apple.quarantine` attribute and Gatekeeper refuses the first open
("cannot verify the developer", or on Sequoia+ the misleading "is damaged").
The user gets past it once with:

- System Settings → Privacy & Security → **Open Anyway** (Sequoia removed the
  right-click → Open shortcut), or
- `xattr -d com.apple.quarantine /Applications/Tessera.app`.

This is the same situation as Windows today: the NSIS installer is unsigned and
SmartScreen shows an equivalent warning; no code-signing certificate was bought
for that.

**Recommendation:** ship unsigned, document "Open Anyway" in the README, and
publish a Homebrew cask (`brew install --cask`) — the cask route does not apply
quarantine and is the normal path for open-source macOS tools. Revisit the paid
account only if the friction produces real user complaints. If it is ever
bought, the CI secrets are `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific) and
`APPLE_TEAM_ID`; Tauri's bundler then signs and notarizes on its own.

**To verify with the first real release:** that the updater can replace an
ad-hoc-signed `.app` without Gatekeeper re-quarantining it. The updater
downloads the `.tar.gz` itself, not through a browser, so quarantine should not
be applied — but this needs a real tag, not theory. If it fails, omit the
`darwin-*` keys from `latest.json` and macOS users are told to download manually.

### 5. CI (`.github/workflows/ci.yml`)

Optional: add a `macos-latest` job running `cargo test`. Cheap on a public repository; worth it so a macOS-only regression shows up before a release, given the Windows disclaimer already in the README.

### 6. Documentation (`README.md`)

Badges, tagline, "Install" section (`.dmg`), updater table row, the vault location (`~/Library/Application Support/tessera/vault.bin`), build prerequisites (Xcode Command Line Tools), and a "new in 0.5.0" caveat like the one for Windows.

Not affected: `scripts/deploy-app.sh` (an internal Linux `.deb` deployment helper).

## Estimate

| Item | Effort |
|---|---|
| Code change + test (`update.rs`, config strings) | ~1 h |
| Release job + manifest script | ~2–3 h including a tag dry-run |
| README | ~1 h |
| Homebrew cask (optional) | ~1–2 h |
| Signing/notarization (optional, only if an Apple account is ever bought) | ~2–4 h |

macOS support can ship in the next release with no Apple account. Signing/notarization is a later, optional polish.
