# Tessera Interface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the vault into an application someone opens twenty times a day — set a password, unlock, find an account, copy its code, and get out.

**Architecture:** React over the command surface from the vault plan. One state hook owns whether the vault is locked and what the accounts are; screens are pure functions of that. A single one-second tick drives every countdown, rather than one timer per row.

**Tech Stack:** React 19, TypeScript 5.8, Vite 7, plain CSS with custom properties. `@tauri-apps/plugin-clipboard-manager` for copying.

**Spec:** `docs/superpowers/specs/2026-08-20-tessera-design.md`

## Global Constraints

- **License:** GPL-3.0-only. **Language:** English throughout, including every string a user reads.
- **Visual direction is the conventional one, chosen deliberately:** dark list on `#0e1117`, a circular countdown ring per row, a single blue accent `#3d7dff`. No per-account colours, no invented metaphor.
- **No web fonts.** The content security policy is `default-src 'self'`, so Google Fonts cannot load, and bundling font files buys little for an interface that is mostly digits. Use the system stack with `font-variant-numeric: tabular-nums`, which is what stops digits from shifting as they change.
- **The front end never holds a secret.** It renders `AccountView` and nothing else.
- **Saving runs Argon2** — 50 to 100 ms per write. Forms save on submit, never on keystroke.
- **Two ring rules, decided before the component is written:** an HOTP row has no ring at all, because its code does not expire; and `seconds_remaining === period` means a *full* ring, not an expiring one, because that is the instant a fresh code begins.
- **Quality floor:** every control reachable by keyboard with a visible focus ring, `prefers-reduced-motion` respected, nothing depending on colour alone.
- **TDD applies to the Rust in Task 1.** The React is verified by `tsc`, by `npm run build`, and by running the app.

---

### Task 1: Settings the user can change

The spec calls the auto-lock timeout configurable and the vault plan left it a constant. This closes that, and adds the clipboard timeout alongside it because both belong to the same screen.

Settings live inside the vault document, which means they are encrypted and — once synchronisation exists — travel between machines for free.

**Files:**
- Create: `src-tauri/src/vault/settings.rs`
- Modify: `src-tauri/src/vault/document.rs`, `src-tauri/src/vault/mod.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct Settings { pub idle_timeout_secs: u32, pub clipboard_clear_secs: u32 }` — `Clone`, `Copy`, `Debug`, `PartialEq`, `Serialize`, `Deserialize`, `Default`
  - `Settings::validated(self) -> Settings`
  - `VaultDocument.settings: Settings`
  - Commands `get_settings` and `set_settings`
  - `VaultManager::idle_timeout(&self) -> Duration`

- [ ] **Step 1: Write the failing tests**

```bash
cat > src-tauri/src/vault/settings.rs <<'EOF'
//! Preferences the user can change.
//!
//! These live inside the sealed document rather than in a config file: they are
//! nobody's business but the user's, and once synchronisation exists they
//! travel between machines without any extra plumbing.

use serde::{Deserialize, Serialize};

/// Five minutes is long enough not to nag and short enough to matter on a
/// laptop left open in a café.
const DEFAULT_IDLE_TIMEOUT_SECS: u32 = 300;
const MIN_IDLE_TIMEOUT_SECS: u32 = 15;
const MAX_IDLE_TIMEOUT_SECS: u32 = 86_400;

/// Long enough to switch windows and paste, short enough that a forgotten code
/// does not sit in the clipboard all afternoon.
const DEFAULT_CLIPBOARD_CLEAR_SECS: u32 = 20;
const MIN_CLIPBOARD_CLEAR_SECS: u32 = 5;
const MAX_CLIPBOARD_CLEAR_SECS: u32 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub idle_timeout_secs: u32,
    pub clipboard_clear_secs: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
            clipboard_clear_secs: DEFAULT_CLIPBOARD_CLEAR_SECS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_documented_ones() {
        let s = Settings::default();
        assert_eq!(s.idle_timeout_secs, 300);
        assert_eq!(s.clipboard_clear_secs, 20);
    }

    #[test]
    fn values_are_clamped_rather_than_refused() {
        // These arrive from the vault document, so a hand-edited or synced file
        // can carry nonsense. A zero timeout would lock the vault between the
        // unlock and the first keystroke, which looks like the app is broken.
        let absurd = Settings {
            idle_timeout_secs: 0,
            clipboard_clear_secs: 0,
        }
        .validated();
        assert_eq!(absurd.idle_timeout_secs, MIN_IDLE_TIMEOUT_SECS);
        assert_eq!(absurd.clipboard_clear_secs, MIN_CLIPBOARD_CLEAR_SECS);

        let enormous = Settings {
            idle_timeout_secs: u32::MAX,
            clipboard_clear_secs: u32::MAX,
        }
        .validated();
        assert_eq!(enormous.idle_timeout_secs, MAX_IDLE_TIMEOUT_SECS);
        assert_eq!(enormous.clipboard_clear_secs, MAX_CLIPBOARD_CLEAR_SECS);
    }

    #[test]
    fn a_sensible_value_passes_through_untouched() {
        let chosen = Settings {
            idle_timeout_secs: 60,
            clipboard_clear_secs: 45,
        };
        assert_eq!(chosen.validated(), chosen);
    }
}
EOF
```

Add to `src-tauri/src/vault/document.rs`, in the `VaultDocument` struct:

```rust
    /// `serde(default)` so a vault written before settings existed still opens.
    #[serde(default)]
    pub settings: Settings,
```

and `settings: Settings::default(),` to `VaultDocument::new`, plus `use crate::vault::Settings;` at the top.

Add this test to `document.rs`'s test module:

```rust
    #[test]
    fn a_document_written_before_settings_existed_still_opens() {
        // Vaults on disk predate this field. Losing them to a missing key would
        // be the worst possible bug in a program that holds second factors.
        let legacy = r#"{"version":1,"device_id":"00000000-0000-4000-8000-000000000000","accounts":[]}"#;
        let doc: VaultDocument = serde_json::from_str(legacy).unwrap();
        assert_eq!(doc.settings, crate::vault::Settings::default());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test vault:: 2>&1 | grep -E '^error' | head -3`
Expected: FAIL — `Settings` and `validated` do not exist.

- [ ] **Step 3: Write the implementation**

Insert into `settings.rs`, above the `#[cfg(test)]` block:

```rust
impl Settings {
    /// Clamp rather than reject.
    ///
    /// These values come out of the vault document, which may have been edited
    /// by hand or written by a future version. Refusing the whole document over
    /// a bad preference would lock the user out of their accounts; clamping
    /// costs them a setting they can change back.
    pub fn validated(self) -> Self {
        Self {
            idle_timeout_secs: self
                .idle_timeout_secs
                .clamp(MIN_IDLE_TIMEOUT_SECS, MAX_IDLE_TIMEOUT_SECS),
            clipboard_clear_secs: self
                .clipboard_clear_secs
                .clamp(MIN_CLIPBOARD_CLEAR_SECS, MAX_CLIPBOARD_CLEAR_SECS),
        }
    }
}
```

Add `mod settings;` and `pub use settings::Settings;` to `src-tauri/src/vault/mod.rs`.

- [ ] **Step 4: Replace the hardcoded timeout**

In `src-tauri/src/vault/manager.rs`, add:

```rust
    /// How long this vault stays open with nothing happening.
    ///
    /// Locked vaults report the default, because there is nothing to lock and
    /// the caller only needs a number to sleep on.
    pub fn idle_timeout(&self) -> Duration {
        let secs = self
            .state
            .as_ref()
            .map(|s| s.document.settings.validated().idle_timeout_secs)
            .unwrap_or(Settings::default().idle_timeout_secs);
        Duration::from_secs(u64::from(secs))
    }
```

and `use crate::vault::Settings;` to its imports.

In `src-tauri/src/commands.rs`, delete `const IDLE_TIMEOUT` and change `poll_idle_lock`:

```rust
#[tauri::command]
pub fn poll_idle_lock(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let mut guard = vault(&state)?;
    let timeout = guard.idle_timeout();
    Ok(guard.lock_if_idle(timeout))
}
```

Add the two settings commands to `commands.rs`:

```rust
#[tauri::command]
pub fn get_settings(state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    let guard = vault(&state)?;
    Ok(guard.document().map_err(fail)?.settings.validated())
}

#[tauri::command]
pub fn set_settings(
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    let clean = settings.validated();
    vault(&state)?
        .mutate(|doc| doc.settings = clean)
        .map_err(fail)?;
    Ok(clean)
}
```

Add `Settings` to the `use crate::vault::{...}` line, and register both commands in `lib.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src-tauri && cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | grep 'test result'`
Expected: clean, 70 tests passing.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(vault): make the auto-lock and clipboard timeouts configurable

The spec calls these configurable and the vault plan left the first a
constant. They live inside the sealed document, so they are encrypted and
will travel between machines once synchronisation exists. Values are clamped
rather than rejected: refusing a whole vault over a bad preference would lock
someone out of their accounts."
```

---

### Task 2: The visual system

Every colour, size and duration in one place, so the screens that follow have nothing to invent.

**Files:**
- Rewrite: `src/app.css`

**Interfaces:**
- Produces the custom properties and base classes the later tasks use.

- [ ] **Step 1: Write the stylesheet**

```bash
cat > src/app.css <<'EOF'
/*
 * Tessera — visual system.
 *
 * The direction is deliberately the conventional one for this category: a dark
 * list, a circular countdown ring per row, and one accent colour. Familiarity
 * is the point; the care goes into rhythm, states and legibility rather than
 * into a house style.
 *
 * No web fonts: the content security policy is default-src 'self', and an
 * interface that is mostly digits gains little from a bundled face. What does
 * matter is tabular figures, so a changing code does not shift the row.
 */

:root {
  /* Surfaces, from the page up to a raised row. */
  --bg: #0e1117;
  --surface: #161b24;
  --surface-hover: #1c222d;
  --line: #232a36;

  /* Text. */
  --text: #e6e9ef;
  --text-dim: #949cad;
  --text-faint: #626b7d;

  /* One accent, used for focus, the ring, and the primary action. */
  --accent: #3d7dff;
  --accent-hover: #5590ff;
  --accent-quiet: rgba(61, 125, 255, 0.14);

  /* The ring turns amber for the last few seconds. Paired with a shrinking
     arc, so the warning never rests on colour alone. */
  --warn: #e0a33c;
  --danger: #e0563c;

  --radius: 10px;
  --radius-sm: 7px;

  /* One rhythm, used everywhere. */
  --gap-1: 4px;
  --gap-2: 8px;
  --gap-3: 12px;
  --gap-4: 16px;
  --gap-5: 24px;
  --gap-6: 32px;

  --font: system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", sans-serif;
  --font-mono: ui-monospace, "Cascadia Code", "Source Code Pro", Menlo, Consolas,
    "DejaVu Sans Mono", monospace;

  --speed: 140ms;

  color-scheme: dark;
}

@media (prefers-reduced-motion: reduce) {
  :root {
    --speed: 0ms;
  }
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: var(--font);
  font-size: 14px;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
  user-select: none;
}

/* Nothing in this application is draggable or selectable except the code,
   which is handled by the copy button, and text the user typed. */
input,
textarea {
  user-select: text;
}

button {
  font: inherit;
  color: inherit;
  background: none;
  border: none;
  cursor: pointer;
}

/* A single, visible focus treatment. Keyboard users get the same affordance
   everywhere rather than whatever each control's default happens to be. */
:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
}

:focus:not(:focus-visible) {
  outline: none;
}

/* ---------- shell ---------- */

.shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}

.shell__body {
  flex: 1;
  overflow-y: auto;
  overscroll-behavior: contain;
}

/* ---------- header ---------- */

.header {
  display: flex;
  align-items: center;
  gap: var(--gap-2);
  padding: var(--gap-4) var(--gap-4) var(--gap-3);
  border-bottom: 1px solid var(--line);
}

.header__title {
  flex: 1;
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  letter-spacing: 0.01em;
}

.header__action {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
  color: var(--text-dim);
  transition: background var(--speed), color var(--speed);
}

.header__action:hover {
  background: var(--surface-hover);
  color: var(--text);
}

/* ---------- search ---------- */

.search {
  padding: var(--gap-3) var(--gap-4);
}

.field {
  width: 100%;
  padding: 9px 12px;
  color: var(--text);
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  font: inherit;
  transition: border-color var(--speed), background var(--speed);
}

.field::placeholder {
  color: var(--text-faint);
}

.field:focus {
  outline: none;
  border-color: var(--accent);
  background: var(--surface-hover);
}

.field--mono {
  font-family: var(--font-mono);
  letter-spacing: 0.04em;
}

/* ---------- account rows ---------- */

.rows {
  list-style: none;
  margin: 0;
  padding: 0 var(--gap-3) var(--gap-5);
}

.row {
  display: flex;
  align-items: center;
  gap: var(--gap-3);
  width: 100%;
  padding: var(--gap-3);
  text-align: left;
  border-radius: var(--radius);
  transition: background var(--speed);
}

.row:hover {
  background: var(--surface);
}

.row__identity {
  flex: 1;
  min-width: 0;
}

.row__issuer {
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.row__label {
  color: var(--text-dim);
  font-size: 12.5px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.row__code {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  font-size: 22px;
  font-weight: 500;
  letter-spacing: 0.08em;
  transition: color var(--speed);
}

.row__code--copied {
  color: var(--accent);
}

.row__trailing {
  display: flex;
  align-items: center;
  gap: var(--gap-2);
}

.row__edit {
  color: var(--text-faint);
  opacity: 0;
  transition: opacity var(--speed), color var(--speed);
}

.row:hover .row__edit,
.row__edit:focus-visible {
  opacity: 1;
}

.row__edit:hover {
  color: var(--text);
}

/* ---------- countdown ring ---------- */

.ring {
  display: block;
  flex: none;
  transform: rotate(-90deg);
}

.ring__track {
  stroke: var(--line);
}

.ring__arc {
  stroke: var(--accent);
  stroke-linecap: round;
  transition: stroke-dashoffset 1s linear, stroke var(--speed);
}

.ring__arc--warn {
  stroke: var(--warn);
}

/* ---------- centred screens ---------- */

.pane {
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: var(--gap-4);
  min-height: 100vh;
  padding: var(--gap-6) var(--gap-5);
}

.pane__title {
  margin: 0;
  font-size: 19px;
  font-weight: 600;
}

.pane__hint {
  margin: 0;
  color: var(--text-dim);
}

.pane__form {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3);
}

/* ---------- buttons ---------- */

.button {
  padding: 9px 14px;
  border-radius: var(--radius-sm);
  font-weight: 550;
  transition: background var(--speed), color var(--speed), opacity var(--speed);
}

.button--primary {
  background: var(--accent);
  color: #fff;
}

.button--primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.button--quiet {
  color: var(--text-dim);
}

.button--quiet:hover:not(:disabled) {
  background: var(--surface-hover);
  color: var(--text);
}

.button--danger {
  color: var(--danger);
}

.button--danger:hover:not(:disabled) {
  background: rgba(224, 86, 60, 0.12);
}

.button:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.button-row {
  display: flex;
  gap: var(--gap-2);
  justify-content: flex-end;
}

/* ---------- messages ---------- */

.error {
  margin: 0;
  padding: var(--gap-2) var(--gap-3);
  color: var(--danger);
  background: rgba(224, 86, 60, 0.1);
  border-radius: var(--radius-sm);
  font-size: 13px;
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--gap-3);
  padding: var(--gap-6) var(--gap-5);
  text-align: center;
  color: var(--text-dim);
}

/* A single toast, bottom-centred, that says what happened and gets out. */
.toast {
  position: fixed;
  left: 50%;
  bottom: var(--gap-5);
  transform: translateX(-50%);
  padding: 8px 14px;
  color: var(--text);
  background: var(--surface-hover);
  border: 1px solid var(--line);
  border-radius: 999px;
  font-size: 13px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
}

/* ---------- settings ---------- */

.setting {
  display: flex;
  flex-direction: column;
  gap: var(--gap-1);
}

.setting__label {
  font-weight: 550;
}

.setting__hint {
  color: var(--text-faint);
  font-size: 12.5px;
}
EOF
```

- [ ] **Step 2: Verify the build still runs**

Run: `npm run build`
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(ui): add the visual system

Conventional direction, chosen deliberately: dark list, one accent, a ring
per row. No web fonts — the CSP is default-src 'self' and an interface that
is mostly digits gains little from a bundled face; tabular figures are what
actually matter, and the system stack has them."
```

---

### Task 3: Talking to the core

Typed bindings and one hook that owns application state.

**Files:**
- Create: `src/lib/api.ts`, `src/lib/useVault.ts`

**Interfaces:**
- Produces the `AccountView`, `VaultStatus` and `Settings` types, one function per command, and `useVault()` returning `{ status, accounts, settings, error, actions }`.

- [ ] **Step 1: Write the bindings**

```bash
mkdir -p src/lib
cat > src/lib/api.ts <<'EOF'
import { invoke } from "@tauri-apps/api/core";

export type AccountKind = "totp" | "hotp" | "steam";

/**
 * One row, as the core renders it. There is deliberately no secret here: the
 * interface has no use for one and no way to hold it safely.
 */
export interface AccountView {
  id: string;
  issuer: string;
  label: string;
  group: string | null;
  kind: AccountKind;
  code: string;
  /** Zero for HOTP, which does not expire. */
  secondsRemaining: number;
  period: number;
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
}

export interface Settings {
  idleTimeoutSecs: number;
  clipboardClearSecs: number;
}

interface RawAccountView {
  id: string;
  issuer: string;
  label: string;
  group: string | null;
  kind: AccountKind;
  code: string;
  seconds_remaining: number;
  period: number;
}

interface RawSettings {
  idle_timeout_secs: number;
  clipboard_clear_secs: number;
}

const toAccount = (raw: RawAccountView): AccountView => ({
  id: raw.id,
  issuer: raw.issuer,
  label: raw.label,
  group: raw.group,
  kind: raw.kind,
  code: raw.code,
  secondsRemaining: raw.seconds_remaining,
  period: raw.period,
});

const toSettings = (raw: RawSettings): Settings => ({
  idleTimeoutSecs: raw.idle_timeout_secs,
  clipboardClearSecs: raw.clipboard_clear_secs,
});

const fromSettings = (s: Settings): RawSettings => ({
  idle_timeout_secs: s.idleTimeoutSecs,
  clipboard_clear_secs: s.clipboardClearSecs,
});

export const vaultStatus = () => invoke<VaultStatus>("vault_status");
export const createVault = (password: string) =>
  invoke<void>("create_vault", { password });
export const unlockVault = (password: string) =>
  invoke<void>("unlock_vault", { password });
export const lockVault = () => invoke<void>("lock_vault");

export const listAccounts = async (): Promise<AccountView[]> =>
  (await invoke<RawAccountView[]>("list_accounts")).map(toAccount);

export const addAccountFromUri = (uri: string) =>
  invoke<void>("add_account_from_uri", { uri });

export type ManualAccount = {
  issuer: string;
  label: string;
  secret: string;
  kind: AccountKind;
  algorithm: "SHA1" | "SHA256" | "SHA512";
  digits: number;
  period: number;
};

export const addAccountManual = (account: ManualAccount) =>
  invoke<void>("add_account_manual", { ...account });

export const updateAccount = (
  id: string,
  issuer: string,
  label: string,
  group: string | null,
) => invoke<void>("update_account", { id, issuer, label, group });

export const deleteAccount = (id: string) =>
  invoke<void>("delete_account", { id });

export const pollIdleLock = () => invoke<boolean>("poll_idle_lock");
export const noteActivity = () => invoke<void>("note_activity");

export const getSettings = async (): Promise<Settings> =>
  toSettings(await invoke<RawSettings>("get_settings"));

export const setSettings = async (settings: Settings): Promise<Settings> =>
  toSettings(
    await invoke<RawSettings>("set_settings", {
      settings: fromSettings(settings),
    }),
  );
EOF
```

- [ ] **Step 2: Write the state hook**

```bash
cat > src/lib/useVault.ts <<'EOF'
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "./api";
import type { AccountView, Settings, VaultStatus } from "./api";

/**
 * Application state: whether the vault is open, and what is in it.
 *
 * Codes are refreshed by a single one-second tick rather than one timer per
 * row. Twenty rows means twenty timers drifting apart, and the countdowns stop
 * agreeing with each other.
 */
export function useVault() {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [accounts, setAccounts] = useState<AccountView[]>([]);
  const [settings, setSettingsState] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  const unlocked = status?.unlocked ?? false;
  const unlockedRef = useRef(unlocked);
  unlockedRef.current = unlocked;

  const refreshStatus = useCallback(async () => {
    setStatus(await api.vaultStatus());
  }, []);

  const refreshAccounts = useCallback(async () => {
    setAccounts(await api.listAccounts());
  }, []);

  useEffect(() => {
    refreshStatus().catch((e) => setError(String(e)));
  }, [refreshStatus]);

  // One tick drives every countdown, and doubles as the idle-lock check.
  useEffect(() => {
    if (!unlocked) return;

    let cancelled = false;
    const tick = async () => {
      try {
        if (await api.pollIdleLock()) {
          if (!cancelled) {
            setAccounts([]);
            await refreshStatus();
          }
          return;
        }
        const rows = await api.listAccounts();
        if (!cancelled) setAccounts(rows);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    };

    void tick();
    const timer = window.setInterval(() => void tick(), 1000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [unlocked, refreshStatus]);

  useEffect(() => {
    if (!unlocked) {
      setSettingsState(null);
      return;
    }
    api.getSettings().then(setSettingsState).catch((e) => setError(String(e)));
  }, [unlocked]);

  /** Run a command, surface its message on failure, and refresh what changed. */
  const run = useCallback(
    async (action: () => Promise<unknown>, refresh = true) => {
      setError(null);
      try {
        await action();
        if (refresh && unlockedRef.current) await refreshAccounts();
        return true;
      } catch (e) {
        setError(String(e));
        return false;
      }
    },
    [refreshAccounts],
  );

  const actions = {
    create: async (password: string) => {
      const ok = await run(() => api.createVault(password), false);
      if (ok) await refreshStatus();
      return ok;
    },
    unlock: async (password: string) => {
      const ok = await run(() => api.unlockVault(password), false);
      if (ok) await refreshStatus();
      return ok;
    },
    lock: async () => {
      await run(() => api.lockVault(), false);
      setAccounts([]);
      await refreshStatus();
    },
    addFromUri: (uri: string) => run(() => api.addAccountFromUri(uri)),
    addManual: (account: api.ManualAccount) =>
      run(() => api.addAccountManual(account)),
    update: (id: string, issuer: string, label: string, group: string | null) =>
      run(() => api.updateAccount(id, issuer, label, group)),
    remove: (id: string) => run(() => api.deleteAccount(id)),
    saveSettings: async (next: Settings) => {
      const ok = await run(async () => {
        setSettingsState(await api.setSettings(next));
      }, false);
      return ok;
    },
    noteActivity: () => {
      void api.noteActivity().catch(() => {
        // Losing one activity ping only means the idle timer runs a little
        // early. Not worth interrupting the user over.
      });
    },
    clearError: () => setError(null),
  };

  return { status, accounts, settings, error, actions };
}
EOF
npm run typecheck
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(ui): add typed command bindings and the vault state hook

One tick drives every countdown. Twenty rows with twenty timers drift apart,
and the countdowns stop agreeing with each other."
```

---

### Task 4: The countdown ring

Small enough to be its own file, and load-bearing enough to deserve one. The two rules from the constraints live here.

**Files:**
- Create: `src/components/CountdownRing.tsx`

- [ ] **Step 1: Write the component**

```bash
mkdir -p src/components
cat > src/components/CountdownRing.tsx <<'EOF'
const SIZE = 26;
const STROKE = 2.5;
const RADIUS = (SIZE - STROKE) / 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/** Below this many seconds the arc turns amber. */
const WARN_AT = 5;

interface Props {
  secondsRemaining: number;
  period: number;
}

/**
 * How long the current code has left.
 *
 * Two rules that are easy to get wrong:
 *
 * `secondsRemaining === period` is the instant a fresh code begins, so it draws
 * a *full* ring. Read naively it looks like the maximum and gets rendered as
 * about-to-expire, and the ring flickers once every period.
 *
 * The warning is an amber arc *and* a shrinking one, so it never rests on
 * colour alone.
 */
export default function CountdownRing({ secondsRemaining, period }: Props) {
  const fraction = period > 0 ? Math.min(secondsRemaining / period, 1) : 0;
  const warning = secondsRemaining > 0 && secondsRemaining <= WARN_AT;

  return (
    <svg
      className="ring"
      width={SIZE}
      height={SIZE}
      viewBox={`0 0 ${SIZE} ${SIZE}`}
      aria-hidden="true"
    >
      <circle
        className="ring__track"
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={RADIUS}
        fill="none"
        strokeWidth={STROKE}
      />
      <circle
        className={`ring__arc${warning ? " ring__arc--warn" : ""}`}
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={RADIUS}
        fill="none"
        strokeWidth={STROKE}
        strokeDasharray={CIRCUMFERENCE}
        strokeDashoffset={CIRCUMFERENCE * (1 - fraction)}
      />
    </svg>
  );
}
EOF
```

- [ ] **Step 2: Typecheck and commit**

```bash
npm run typecheck
git add -A
git commit -m "feat(ui): add the countdown ring

A full period means a full ring — it is the instant a fresh code begins, not
the moment before one expires, and reading it the other way makes the ring
flicker once every period. The warning is a shrinking amber arc rather than a
colour change alone."
```

---

### Task 5: The unlock and first-run screens

The first thing anyone sees. The first-run screen is also the only place Tessera can be honest about what a lost master password means, so it says it there.

**Files:**
- Create: `src/screens/Unlock.tsx`

- [ ] **Step 1: Write the screen**

```bash
mkdir -p src/screens
cat > src/screens/Unlock.tsx <<'EOF'
import { useState } from "react";

interface Props {
  /** A vault already exists, so this is an unlock rather than a first run. */
  existing: boolean;
  error: string | null;
  onSubmit: (password: string) => Promise<boolean>;
}

/** The shortest password worth allowing on a file that holds second factors. */
const MIN_LENGTH = 8;

export default function Unlock({ existing, error, onSubmit }: Props) {
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [busy, setBusy] = useState(false);

  const tooShort = !existing && password.length > 0 && password.length < MIN_LENGTH;
  const mismatched =
    !existing && confirmation.length > 0 && confirmation !== password;
  const ready = existing
    ? password.length > 0
    : password.length >= MIN_LENGTH && confirmation === password;

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!ready || busy) return;
    setBusy(true);
    const ok = await onSubmit(password);
    setBusy(false);
    if (ok) {
      setPassword("");
      setConfirmation("");
    }
  };

  return (
    <div className="pane">
      <div>
        <h1 className="pane__title">
          {existing ? "Unlock Tessera" : "Set a master password"}
        </h1>
        <p className="pane__hint">
          {existing
            ? "Your accounts are encrypted with this password."
            : "This password encrypts your accounts. Tessera cannot recover it — if you lose it, you lose the vault."}
        </p>
      </div>

      <form className="pane__form" onSubmit={submit}>
        <input
          className="field"
          type="password"
          autoFocus
          autoComplete={existing ? "current-password" : "new-password"}
          placeholder="Master password"
          aria-label="Master password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />

        {!existing && (
          <input
            className="field"
            type="password"
            autoComplete="new-password"
            placeholder="Repeat it"
            aria-label="Repeat the master password"
            value={confirmation}
            onChange={(e) => setConfirmation(e.target.value)}
          />
        )}

        {tooShort && (
          <p className="error">Use at least {MIN_LENGTH} characters.</p>
        )}
        {mismatched && <p className="error">These two do not match.</p>}
        {error && <p className="error">{error}</p>}

        <button
          className="button button--primary"
          type="submit"
          disabled={!ready || busy}
        >
          {existing ? "Unlock" : "Create vault"}
        </button>
      </form>
    </div>
  );
}
EOF
npm run typecheck
```

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "feat(ui): add the unlock and first-run screens

The first-run screen is the only place Tessera can say what a lost master
password costs, so it says it there rather than burying it in a README."
```

---

### Task 6: Adding an account

Two routes, in the order a desktop user reaches for them: paste the link, or type the fields.

**Files:**
- Create: `src/screens/AddAccount.tsx`

- [ ] **Step 1: Write the screen**

```bash
cat > src/screens/AddAccount.tsx <<'EOF'
import { useState } from "react";

import type { ManualAccount } from "../lib/api";

interface Props {
  error: string | null;
  onPaste: (uri: string) => Promise<boolean>;
  onManual: (account: ManualAccount) => Promise<boolean>;
  onDone: () => void;
}

type Mode = "link" | "manual";

export default function AddAccount({ error, onPaste, onManual, onDone }: Props) {
  const [mode, setMode] = useState<Mode>("link");
  const [uri, setUri] = useState("");
  const [issuer, setIssuer] = useState("");
  const [label, setLabel] = useState("");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);

  // Saving derives a key with Argon2, which takes a tenth of a second. That is
  // fine once per submission and unusable per keystroke, so nothing here saves
  // as you type.
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    const ok =
      mode === "link"
        ? await onPaste(uri.trim())
        : await onManual({
            issuer: issuer.trim(),
            label: label.trim(),
            secret: secret.trim(),
            kind: "totp",
            algorithm: "SHA1",
            digits: 6,
            period: 30,
          });
    setBusy(false);
    if (ok) onDone();
  };

  const ready =
    mode === "link" ? uri.trim().length > 0 : secret.trim().length > 0 && label.trim().length > 0;

  return (
    <div className="pane">
      <div>
        <h1 className="pane__title">Add an account</h1>
        <p className="pane__hint">
          {mode === "link"
            ? "Paste the otpauth:// link behind the QR code the service showed you."
            : "Type what the service gave you. Most services use these defaults."}
        </p>
      </div>

      <form className="pane__form" onSubmit={submit}>
        {mode === "link" ? (
          <input
            className="field field--mono"
            autoFocus
            placeholder="otpauth://totp/..."
            aria-label="otpauth link"
            value={uri}
            onChange={(e) => setUri(e.target.value)}
          />
        ) : (
          <>
            <input
              className="field"
              autoFocus
              placeholder="Service, for example GitHub"
              aria-label="Service"
              value={issuer}
              onChange={(e) => setIssuer(e.target.value)}
            />
            <input
              className="field"
              placeholder="Account, for example your email"
              aria-label="Account"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
            />
            <input
              className="field field--mono"
              placeholder="Secret key"
              aria-label="Secret key"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
            />
          </>
        )}

        {error && <p className="error">{error}</p>}

        <div className="button-row">
          <button
            className="button button--quiet"
            type="button"
            onClick={() => setMode(mode === "link" ? "manual" : "link")}
          >
            {mode === "link" ? "Type it instead" : "Paste a link instead"}
          </button>
          <button className="button button--quiet" type="button" onClick={onDone}>
            Cancel
          </button>
          <button
            className="button button--primary"
            type="submit"
            disabled={!ready || busy}
          >
            Add
          </button>
        </div>
      </form>
    </div>
  );
}
EOF
npm run typecheck
```

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "feat(ui): add the add-account screen

Nothing saves as you type: a write derives a key with Argon2, which is fine
once per submission and unusable per keystroke."
```

---

### Task 7: Editing and deleting

**Files:**
- Create: `src/screens/EditAccount.tsx`

- [ ] **Step 1: Write the screen**

```bash
cat > src/screens/EditAccount.tsx <<'EOF'
import { useState } from "react";

import type { AccountView } from "../lib/api";

interface Props {
  account: AccountView;
  error: string | null;
  onSave: (issuer: string, label: string, group: string | null) => Promise<boolean>;
  onDelete: () => Promise<boolean>;
  onDone: () => void;
}

export default function EditAccount({
  account,
  error,
  onSave,
  onDelete,
  onDone,
}: Props) {
  const [issuer, setIssuer] = useState(account.issuer);
  const [label, setLabel] = useState(account.label);
  const [group, setGroup] = useState(account.group ?? "");
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [busy, setBusy] = useState(false);

  const save = async (event: React.FormEvent) => {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    const ok = await onSave(issuer.trim(), label.trim(), group.trim() || null);
    setBusy(false);
    if (ok) onDone();
  };

  const remove = async () => {
    setBusy(true);
    const ok = await onDelete();
    setBusy(false);
    if (ok) onDone();
  };

  return (
    <div className="pane">
      <div>
        <h1 className="pane__title">Edit account</h1>
        <p className="pane__hint">
          The secret cannot be changed. To replace it, add the account again and
          delete this one.
        </p>
      </div>

      <form className="pane__form" onSubmit={save}>
        <input
          className="field"
          autoFocus
          placeholder="Service"
          aria-label="Service"
          value={issuer}
          onChange={(e) => setIssuer(e.target.value)}
        />
        <input
          className="field"
          placeholder="Account"
          aria-label="Account"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
        <input
          className="field"
          placeholder="Group, optional"
          aria-label="Group"
          value={group}
          onChange={(e) => setGroup(e.target.value)}
        />

        {error && <p className="error">{error}</p>}

        {confirmingDelete ? (
          <div className="button-row">
            <p className="pane__hint" style={{ flex: 1 }}>
              Delete {account.issuer || account.label}? You will lose access to
              whatever it protects unless you have another copy.
            </p>
            <button
              className="button button--quiet"
              type="button"
              onClick={() => setConfirmingDelete(false)}
            >
              Keep it
            </button>
            <button
              className="button button--danger"
              type="button"
              onClick={remove}
              disabled={busy}
            >
              Delete
            </button>
          </div>
        ) : (
          <div className="button-row">
            <button
              className="button button--danger"
              type="button"
              onClick={() => setConfirmingDelete(true)}
              style={{ marginRight: "auto" }}
            >
              Delete
            </button>
            <button className="button button--quiet" type="button" onClick={onDone}>
              Cancel
            </button>
            <button className="button button--primary" type="submit" disabled={busy}>
              Save
            </button>
          </div>
        )}
      </form>
    </div>
  );
}
EOF
npm run typecheck
```

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "feat(ui): add the edit and delete screen

Deleting asks first and says what is actually at stake, because the thing
being deleted is what stands between the user and an account they own."
```

---

### Task 8: The list, settings, and the shell that holds them

Where it all becomes an application.

**Files:**
- Create: `src/screens/AccountList.tsx`, `src/screens/SettingsScreen.tsx`
- Rewrite: `src/App.tsx`

- [ ] **Step 1: Write the account list**

```bash
cat > src/screens/AccountList.tsx <<'EOF'
import { useMemo, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

import CountdownRing from "../components/CountdownRing";
import type { AccountView } from "../lib/api";

interface Props {
  accounts: AccountView[];
  clipboardClearSecs: number;
  onEdit: (account: AccountView) => void;
  onActivity: () => void;
}

export default function AccountList({
  accounts,
  clipboardClearSecs,
  onEdit,
  onActivity,
}: Props) {
  const [query, setQuery] = useState("");
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return accounts;
    return accounts.filter((a) =>
      `${a.issuer} ${a.label} ${a.group ?? ""}`.toLowerCase().includes(needle),
    );
  }, [accounts, query]);

  const copy = async (account: AccountView) => {
    onActivity();
    await writeText(account.code);
    setCopiedId(account.id);
    setToast(`Copied. Clears in ${clipboardClearSecs}s.`);
    window.setTimeout(() => setCopiedId(null), 1200);
    window.setTimeout(() => setToast(null), 2200);

    // Clearing only holds if Tessera still owns the clipboard. Some Wayland
    // compositors hand ownership to whatever copied last, in which case the
    // user's own paste has already replaced this and there is nothing to clear.
    window.setTimeout(() => {
      writeText("").catch(() => {});
    }, clipboardClearSecs * 1000);
  };

  if (accounts.length === 0) {
    return (
      <div className="empty">
        <p>No accounts yet.</p>
        <p>
          Add one by pasting the otpauth:// link behind a service's QR code, or
          by typing the secret it gave you.
        </p>
      </div>
    );
  }

  return (
    <>
      <div className="search">
        <input
          className="field"
          type="search"
          placeholder="Search"
          aria-label="Search accounts"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            onActivity();
          }}
        />
      </div>

      {matches.length === 0 ? (
        <div className="empty">
          <p>Nothing matches “{query.trim()}”.</p>
        </div>
      ) : (
        <ul className="rows">
          {matches.map((account) => (
            <li key={account.id}>
              <button
                className="row"
                type="button"
                onClick={() => void copy(account)}
                title="Copy code"
              >
                <span className="row__identity">
                  <span className="row__issuer">
                    {account.issuer || account.label}
                  </span>
                  {account.issuer && (
                    <span className="row__label">{account.label}</span>
                  )}
                </span>

                <span
                  className={`row__code${
                    copiedId === account.id ? " row__code--copied" : ""
                  }`}
                >
                  {account.code}
                </span>

                <span className="row__trailing">
                  {/* HOTP has no ring: its code does not expire on a clock. */}
                  {account.kind !== "hotp" && (
                    <CountdownRing
                      secondsRemaining={account.secondsRemaining}
                      period={account.period}
                    />
                  )}
                </span>
              </button>

              <button
                className="row__edit"
                type="button"
                onClick={() => onEdit(account)}
                aria-label={`Edit ${account.issuer || account.label}`}
                style={{ position: "absolute", right: 0, top: 0, padding: 8 }}
              >
                ⋯
              </button>
            </li>
          ))}
        </ul>
      )}

      {toast && <div className="toast">{toast}</div>}
    </>
  );
}
EOF
```

The edit control needs its row to be a positioning context, so add to `src/app.css`:

```css
.rows li {
  position: relative;
}
```

- [ ] **Step 2: Write the settings screen**

```bash
cat > src/screens/SettingsScreen.tsx <<'EOF'
import { useState } from "react";

import type { Settings } from "../lib/api";

interface Props {
  settings: Settings;
  error: string | null;
  onSave: (settings: Settings) => Promise<boolean>;
  onLock: () => void;
  onDone: () => void;
}

const IDLE_CHOICES = [
  { secs: 60, label: "1 minute" },
  { secs: 300, label: "5 minutes" },
  { secs: 900, label: "15 minutes" },
  { secs: 3600, label: "1 hour" },
];

const CLIPBOARD_CHOICES = [
  { secs: 10, label: "10 seconds" },
  { secs: 20, label: "20 seconds" },
  { secs: 60, label: "1 minute" },
];

export default function SettingsScreen({
  settings,
  error,
  onSave,
  onLock,
  onDone,
}: Props) {
  const [draft, setDraft] = useState(settings);
  const [busy, setBusy] = useState(false);

  const save = async () => {
    setBusy(true);
    const ok = await onSave(draft);
    setBusy(false);
    if (ok) onDone();
  };

  return (
    <div className="pane">
      <h1 className="pane__title">Settings</h1>

      <div className="setting">
        <label className="setting__label" htmlFor="idle">
          Lock after
        </label>
        <select
          id="idle"
          className="field"
          value={draft.idleTimeoutSecs}
          onChange={(e) =>
            setDraft({ ...draft, idleTimeoutSecs: Number(e.target.value) })
          }
        >
          {IDLE_CHOICES.map((c) => (
            <option key={c.secs} value={c.secs}>
              {c.label}
            </option>
          ))}
        </select>
        <span className="setting__hint">
          Tessera locks itself when nothing has happened for this long. You will
          need your master password again.
        </span>
      </div>

      <div className="setting">
        <label className="setting__label" htmlFor="clipboard">
          Clear a copied code after
        </label>
        <select
          id="clipboard"
          className="field"
          value={draft.clipboardClearSecs}
          onChange={(e) =>
            setDraft({ ...draft, clipboardClearSecs: Number(e.target.value) })
          }
        >
          {CLIPBOARD_CHOICES.map((c) => (
            <option key={c.secs} value={c.secs}>
              {c.label}
            </option>
          ))}
        </select>
        <span className="setting__hint">
          Some Linux desktops hand the clipboard to whatever copied last, so a
          code may be replaced before Tessera can clear it.
        </span>
      </div>

      {error && <p className="error">{error}</p>}

      <div className="button-row">
        <button
          className="button button--quiet"
          type="button"
          onClick={onLock}
          style={{ marginRight: "auto" }}
        >
          Lock now
        </button>
        <button className="button button--quiet" type="button" onClick={onDone}>
          Cancel
        </button>
        <button
          className="button button--primary"
          type="button"
          onClick={() => void save()}
          disabled={busy}
        >
          Save
        </button>
      </div>
    </div>
  );
}
EOF
```

- [ ] **Step 3: Write the shell**

```bash
cat > src/App.tsx <<'EOF'
import { useState } from "react";

import { useVault } from "./lib/useVault";
import type { AccountView } from "./lib/api";
import AccountList from "./screens/AccountList";
import AddAccount from "./screens/AddAccount";
import EditAccount from "./screens/EditAccount";
import SettingsScreen from "./screens/SettingsScreen";
import Unlock from "./screens/Unlock";

type Screen =
  | { name: "list" }
  | { name: "add" }
  | { name: "edit"; account: AccountView }
  | { name: "settings" };

export default function App() {
  const { status, accounts, settings, error, actions } = useVault();
  const [screen, setScreen] = useState<Screen>({ name: "list" });

  // The very first render, before the core has answered.
  if (!status) return <main className="shell" />;

  if (!status.unlocked) {
    return (
      <main className="shell">
        <Unlock
          existing={status.exists}
          error={error}
          onSubmit={status.exists ? actions.unlock : actions.create}
        />
      </main>
    );
  }

  const back = () => {
    actions.clearError();
    setScreen({ name: "list" });
  };

  if (screen.name === "add") {
    return (
      <main className="shell">
        <AddAccount
          error={error}
          onPaste={actions.addFromUri}
          onManual={actions.addManual}
          onDone={back}
        />
      </main>
    );
  }

  if (screen.name === "edit") {
    // The list refreshes every second, so read the live row rather than the
    // one captured when the screen opened.
    const live =
      accounts.find((a) => a.id === screen.account.id) ?? screen.account;
    return (
      <main className="shell">
        <EditAccount
          account={live}
          error={error}
          onSave={(issuer, label, group) =>
            actions.update(live.id, issuer, label, group)
          }
          onDelete={() => actions.remove(live.id)}
          onDone={back}
        />
      </main>
    );
  }

  if (screen.name === "settings" && settings) {
    return (
      <main className="shell">
        <SettingsScreen
          settings={settings}
          error={error}
          onSave={actions.saveSettings}
          onLock={() => {
            void actions.lock();
            setScreen({ name: "list" });
          }}
          onDone={back}
        />
      </main>
    );
  }

  return (
    <main className="shell">
      <header className="header">
        <h1 className="header__title">Tessera</h1>
        <button
          className="header__action"
          type="button"
          onClick={() => setScreen({ name: "settings" })}
          aria-label="Settings"
          title="Settings"
        >
          ⚙
        </button>
        <button
          className="header__action"
          type="button"
          onClick={() => setScreen({ name: "add" })}
          aria-label="Add an account"
          title="Add an account"
        >
          ＋
        </button>
      </header>

      <div className="shell__body">
        {error && <p className="error" style={{ margin: "12px 16px" }}>{error}</p>}
        <AccountList
          accounts={accounts}
          clipboardClearSecs={settings?.clipboardClearSecs ?? 20}
          onEdit={(account) => setScreen({ name: "edit", account })}
          onActivity={actions.noteActivity}
        />
      </div>
    </main>
  );
}
EOF
npm run typecheck && npm run build
```

- [ ] **Step 4: Add the clipboard permission**

`writeText` needs it. `src-tauri/capabilities/default.json` already lists
`clipboard-manager:allow-write-text`, so no change is required — confirm it is there
rather than assuming.

Run: `grep clipboard src-tauri/capabilities/default.json`
Expected: both `allow-read-text` and `allow-write-text`.

- [ ] **Step 5: Build the application and confirm it runs**

Run: `npm run tauri build -- --bundles deb 2>&1 | tail -3`
Expected: a `.deb` is produced.

If a graphical session is available, run `npm run tauri dev` and walk through:
set a password, add an account by pasting
`otpauth://totp/GitHub:you@example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ`,
watch the ring deplete, click the row to copy, edit it, delete it, and change the
lock timeout in settings.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ui): add the account list, settings, and the application shell

Clicking a row copies its code — the thing you opened the application to do
should be the largest target on screen, not a secondary control."
```

---

## Definition of done

- `cargo test` passes, including the new settings tests.
- `cargo fmt --check`, `cargo clippy -D warnings`, `npm run typecheck` and `npm run build` are all clean.
- CI is green.
- The application can create a vault, add an account from a link, show a code that matches what a phone shows for the same secret, copy it, edit it, delete it, lock, and unlock.
- The auto-lock timeout is changeable from the interface, meeting spec §6.

## What this plan deliberately does not do

Importing from Google Authenticator, decoding QR codes from images, capturing the screen, and Google synchronisation. Those are the next plans. Adding an account here means pasting a link or typing the fields.
