import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import * as api from "../lib/api";

import type { Settings } from "../lib/api";

interface Props {
  settings: Settings;
  updateChecking: boolean;
  currentVersion: string;
  onSetUpdateChecking: (enabled: boolean) => Promise<void>;
  onSetVaultLocation: (folder: string) => Promise<boolean>;
  error: string | null;
  onSave: (settings: Settings) => Promise<boolean>;
  onLock: () => void;
  onDone: () => void;
}

// The vault caps this at 24 hours, so that is where the list stops. Anything
// longer would be silently clamped, which is worse than not offering it.
const IDLE_CHOICES = [
  { secs: 60, label: "1 minute" },
  { secs: 300, label: "5 minutes" },
  { secs: 600, label: "10 minutes" },
  { secs: 1800, label: "30 minutes" },
  { secs: 3600, label: "1 hour" },
  { secs: 14400, label: "4 hours" },
  { secs: 43200, label: "12 hours" },
  { secs: 86400, label: "24 hours" },
];

const CLIPBOARD_CHOICES = [
  { secs: 10, label: "10 seconds" },
  { secs: 20, label: "20 seconds" },
  { secs: 60, label: "1 minute" },
];

export default function SettingsScreen({
  settings,
  updateChecking,
  currentVersion,
  onSetUpdateChecking,
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
          Tessera locks itself when nothing has happened for this long, and asks
          for your master password again. Time the machine spends asleep counts.
          While it is unlocked your accounts are decrypted in memory, so a long
          setting trades the typing for anyone who reaches the machine being able
          to read your codes.
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

      <div className="setting">
        <label className="setting__check">
          <input
            type="checkbox"
            checked={updateChecking}
            onChange={(e) => void onSetUpdateChecking(e.target.checked)}
          />
          <span className="setting__label">Check for updates</span>
        </label>
        <span className="setting__hint">
          Tessera asks GitHub whether a newer release exists, and tells you if
          there is one. It sends nothing about you or your accounts — only a
          request for the release list, the same one your browser would make.
          This is the only network request Tessera ever makes, and turning this
          off means it makes none at all.
          {currentVersion && ` You are on ${currentVersion}.`}
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
