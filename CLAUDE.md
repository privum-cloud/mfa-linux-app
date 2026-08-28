# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Tessera is a desktop authenticator (TOTP/HOTP/Steam) built on Tauri 2 — a Rust core in
`src-tauri/`, a React 19 + TypeScript interface in `src/`. It ships on Linux and Windows.
Everything security-relevant lives in Rust; the interface is a renderer.

**This repository is public and open source.** Write everything — code, comments, commits,
docs — in English. Never commit machine names, IP addresses, personal email addresses,
credentials, or absolute paths from a developer's machine.

## Working agreements

- **Every change goes on a branch and lands through a pull request.** Never commit directly
  to `main`.
- **Commit messages carry no co-authorship or attribution trailers** — no `Co-Authored-By`,
  no session links, no tool advertising. This overrides the default commit template.

## Commands

Run Node commands from the repository root, Cargo commands from `src-tauri/`.

```bash
npm install                  # once
npm run tauri dev            # run the app (starts Vite on :1431, then the Rust shell)
npm run typecheck            # tsc --noEmit
npm run build                # tsc && vite build — what CI runs
```

```bash
cd src-tauri
cargo test --all             # 140 tests
cargo test --lib <substring> # one test, e.g. cargo test --lib a_saved_path_is_read_back
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings   # CI treats warnings as errors
```

Every Rust test is an inline `#[cfg(test)] mod tests` inside the `tessera_lib` crate; there
are no `tests/` directories. **There is no JavaScript test runner** — the frontend is covered
by `npm run typecheck` and `npm run build` only.

Building the Rust crate on Linux needs the libraries Tauri links against:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
  libayatana-appindicator3-dev patchelf
```

The toolchain is pinned in `rust-toolchain.toml` (1.96.0) so a lint added to a newer clippy
cannot fail a commit that was clean when written. Raise it deliberately.

If a build fails with `failed to read plugin permissions: ... No such file or directory`, the
`src-tauri/target/` cache holds absolute paths from a previous location of the checkout.
`cargo clean -p tauri -p tessera` fixes it.

## Releasing

Releases are tag-driven; nothing is built by hand.

```bash
git tag -a vX.Y.Z -m "..." && git push origin vX.Y.Z
```

`.github/workflows/release.yml` then runs **draft → (linux ‖ windows) → publish**. The draft
exists so a half-finished release is never visible, and it is only published once *both*
platforms have uploaded. Linux produces deb/rpm/AppImage, Windows produces an NSIS `-setup.exe`.

**The version lives in three files and must be bumped in all of them**, plus the lockfile:
`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, then
`cargo update -p tessera`.

## Architecture

### Adding a command touches four places

The Rust core and the interface are joined by a hand-written binding, not by codegen. A new
command that misses a step fails at runtime, not at compile time:

1. `src-tauri/src/commands.rs` — the `#[tauri::command] pub fn`
2. `src-tauri/src/lib.rs` — add it to `tauri::generate_handler![...]`
3. `src/lib/api.ts` — a typed wrapper, plus a `RawX` interface and a mapper, because serde
   emits `snake_case` and the interface is `camelCase`
4. `src/lib/useVault.ts` — an action, if the interface needs to call it

### The security boundary

`AccountView` has no field for a secret, and the test asserts that on the *serialised* form
rather than trusting the type — what matters is what crosses the boundary, not what the struct
is called. Keep it that way: the interface receives generated codes and metadata only.

For the same reason the export QR is rendered to PNG **in Rust** and crosses as a
`data:image/png;base64,...` URI. Its payload carries every secret in the vault, so it must
never travel as text.

The CSP is `default-src 'self'` (`src-tauri/tauri.conf.json`). Fonts and assets must be
bundled locally — no CDN, no Google Fonts. `img-src` allows `data:` for the export QR.

### Where state lives, and why it is split

- **The vault** — `~/.local/share/tessera/vault.bin` (Linux, XDG), `%LOCALAPPDATA%` on
  Windows, or a folder the user chose. argon2id → AES-256-GCM; the GCM tag *is* the password
  check. Written to a temporary file and renamed into place, so an interrupted save cannot
  leave a vault that is neither the old one nor the new one.
- **`Settings`** (`vault/settings.rs`) live *inside* the sealed document, so they travel
  between machines for free. Values are **clamped, never rejected** — refusing a whole
  document over a bad preference would lock someone out of their accounts.
- **`Location`** (`vault/location.rs`) is the one preference that cannot live in the vault: the
  path is needed before there is anything to unlock. It sits in `~/.config/tessera/config.json`
  with no secrets in it, and a corrupt config **falls back to the default rather than refusing
  to start**.
- The `device_id` *inside the document* identifies the **vault**, not the machine — it travels
  with the file, so every machine sharing a vault reports the same one. Machine identity is the
  `device_id` in `Location`, which never syncs.

### Sharing one vault between machines

There is no server. The user points Tessera at a folder that already syncs (Drive, Nextcloud,
Syncthing, a network mount) and several machines share one file. That makes every write a
potential concurrent write, which drives three rules in `vault/manager.rs`:

- `mutate` **re-reads and merges before writing**. Change detection is a `stat` (mtime + length)
  against what this manager last saw, so the common case does not pay for another Argon2 pass.
- The temporary file is **named per writer** (machine id + per-manager random). A shared
  `vault.bin.tmp` is one file every writer would use, and two saves at once interleave into a
  rename that produces neither document. Two windows on one machine need the random part.
- `refresh_from_disk` runs on the interface's one-second tick, so an account added elsewhere
  appears on its own.

`sync/merge.rs` holds the merge rules, does no I/O, and is tested **commutative and
idempotent** — merging in either order must reach the same document. The rules:

- `revision` decides; `updated_at` only breaks ties. Wall clock cannot be the authority — a
  laptop with a skewed clock would win every merge forever and nobody could see why.
- **The HOTP counter is resolved separately, and before the winner is picked: `max(mine, theirs)`.**
  Last-write-wins would discard the losing side's increments, leave the token *behind* the
  server, and lock the user out of that account. Being ahead is recoverable through the
  resynchronisation window; being behind is not.
- Tombstones need no special case: `soft_delete` calls `touch`, so a deletion always carries a
  higher revision than the record it removes. A later edit still wins, which is what someone
  who deleted on one machine and edited on another meant.
- Deletion is always a tombstone, never a removal (90-day retention). Without one, a delete
  never propagates and the next merge resurrects the account.

### Codes

`otp/` performs no I/O and holds no state — the most correctness-critical code in the project
and the easiest to test exhaustively, because the RFCs publish their expected outputs. It is
verified against the RFC 6238 and RFC 4226 vectors. `digits` is treated as untrusted (it is
deserialised from the vault and from protobuf someone else wrote); an unclamped exponent here
panics in debug builds.

The idle lock reads **two clocks** and locks if either says so: `Instant` is `CLOCK_MONOTONIC`,
which does not advance while a machine is suspended, so a laptop lid closed with the vault open
used to wake up still open. `SystemTime` counts suspended time but can jump; a backward jump is
treated as zero elapsed, never as a reason to stay open. Generally: **`Instant` alone cannot
back a time-based security policy on a machine that suspends.**

### Import and export

- **Accounts are matched on their secret, not on an id.** An account re-exported from a phone
  arrives with a fresh id each time, and people repeat an import when they are unsure it
  worked — matching on id would duplicate everything on every attempt.
- **Tombstones are excluded from that match.** A deleted account was the user removing it;
  importing the same secret again is them asking for it back.
- **MD5 is refused.** It is in Google's schema and not in Tessera, and an account that
  silently generates wrong codes is worse than one that was refused.
- The Google Authenticator protobuf is **written by hand** (`import/protobuf.rs`) rather than
  generated with `prost`, because `prost-build` requires `protoc` at build time — on CI and on
  every contributor's machine — to generate one message and three enums. The varint is capped
  at 10 bytes and every length is bounds-checked, because the input came from a QR code someone
  else made.
- `qrcode` is used with `default-features = false`. Its `image` feature pulls in its own version
  of the `image` crate, and two versions in the tree means the buffer types stop matching. Keep
  `cargo tree -i image` at one version.
- `image` is built with only the `png` and `jpeg` features, on **both** platforms. Anything else
  a user hands over is rejected as unreadable.

## Notes

`docs/superpowers/` (the design spec and the implementation plans) is **gitignored** — those
files are local to a working copy and a fresh clone will not have them. Do not expect them to
be there, and do not commit them.
