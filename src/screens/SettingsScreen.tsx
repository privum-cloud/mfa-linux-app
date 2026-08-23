import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import * as api from "../lib/api";

import type { Settings } from "../lib/api";

interface Props {
  settings: Settings;
  onSetVaultLocation: (folder: string) => Promise<boolean>;
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
  onSetVaultLocation,
  error,
  onSave,
  onLock,
  onDone,
}: Props) {
  const [draft, setDraft] = useState(settings);
  const [busy, setBusy] = useState(false);
  const [location, setLocation] = useState<api.VaultLocation | null>(null);

  useEffect(() => {
    api.vaultLocation().then(setLocation).catch(() => setLocation(null));
  }, []);

  const chooseFolder = async () => {
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked !== "string") return;
    setBusy(true);
    const ok = await onSetVaultLocation(picked);
    setBusy(false);
    if (ok) {
      onDone();
      return;
    }
    api.vaultLocation().then(setLocation).catch(() => {});
  };

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

      <div className="setting">
        <span className="setting__label">Where the vault lives</span>
        <code className="setting__path">{location?.path ?? "…"}</code>
        <button
          className="button button--quiet setting__action"
          type="button"
          onClick={() => void chooseFolder()}
          disabled={busy}
        >
          Choose a folder
        </button>
        <span className="setting__hint">
          Put the vault in a folder that already syncs — Drive, Nextcloud,
          Syncthing — and your machines will share it. Every machine needs the
          same master password, because the file is sealed with it. Choosing a
          folder that already holds a vault opens that one instead.
        </span>
      </div>

      {error && <p className="error">{error}</p>}

      <div className="button-row">
        <button
          className="button button--quiet button-row__spacer"
          type="button"
          onClick={onLock}
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
