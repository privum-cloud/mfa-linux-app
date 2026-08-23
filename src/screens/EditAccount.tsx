import { useState, type FormEvent } from "react";

import type { AccountView, FolderView } from "../lib/api";

interface Props {
  account: AccountView;
  folders: FolderView[];
  error: string | null;
  onSave: (issuer: string, label: string) => Promise<boolean>;
  onMoveToFolder: (folderId: string | null) => Promise<boolean>;
  onDelete: () => Promise<boolean>;
  onDone: () => void;
}

export default function EditAccount({
  account,
  folders,
  error,
  onSave,
  onMoveToFolder,
  onDelete,
  onDone,
}: Props) {
  const [issuer, setIssuer] = useState(account.issuer);
  const [label, setLabel] = useState(account.label);

  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [busy, setBusy] = useState(false);

  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    const ok = await onSave(issuer.trim(), label.trim());
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

      <form className="pane__form" onSubmit={(e) => void save(e)}>
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
        <label className="setting">
          <span className="setting__label">Folder</span>
          <select
            className="field"
            value={account.folderId ?? ""}
            onChange={(e) => void onMoveToFolder(e.target.value || null)}
          >
            <option value="">No folder</option>
            {folders.map((f) => (
              <option key={f.id} value={f.id}>
                {"\u00a0\u00a0".repeat(f.depth) + f.name}
              </option>
            ))}
          </select>
        </label>

        {error && <p className="error">{error}</p>}

        {confirmingDelete ? (
          <>
            <p className="pane__hint">
              Delete {account.issuer || account.label}? You will lose access to
              whatever it protects unless you have another copy.
            </p>
            <div className="button-row">
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
                onClick={() => void remove()}
                disabled={busy}
              >
                Delete
              </button>
            </div>
          </>
        ) : (
          <div className="button-row">
            <button
              className="button button--danger button-row__spacer"
              type="button"
              onClick={() => setConfirmingDelete(true)}
            >
              Delete
            </button>
            <button className="button button--quiet" type="button" onClick={onDone}>
              Cancel
            </button>
            <button
              className="button button--primary"
              type="submit"
              disabled={busy}
            >
              Save
            </button>
          </div>
        )}
      </form>
    </div>
  );
}
