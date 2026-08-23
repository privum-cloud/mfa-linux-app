import { useState } from "react";

import { FOLDER_ICONS, type FolderView } from "../lib/api";

interface Props {
  folders: FolderView[];
  error: string | null;
  onCreate: (name: string, parentId: string | null) => Promise<boolean>;
  onRename: (id: string, name: string) => Promise<boolean>;
  onSetIcon: (id: string, icon: string | null) => Promise<boolean>;
  onMove: (id: string, parentId: string | null) => Promise<boolean>;
  onRemove: (id: string) => Promise<boolean>;
  onDone: () => void;
}

/** Indent a folder's name in a flat <select>, since options cannot nest. */
const indented = (f: FolderView) => `${"  ".repeat(f.depth)}${f.name}`;

export default function FolderEditor({
  folders,
  error,
  onCreate,
  onRename,
  onSetIcon,
  onMove,
  onRemove,
  onDone,
}: Props) {
  const [newName, setNewName] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");
  const [confirmingRemove, setConfirmingRemove] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const guard = async (action: () => Promise<boolean>) => {
    setBusy(true);
    const ok = await action();
    setBusy(false);
    return ok;
  };

  const create = async () => {
    if (!newName.trim()) return;
    if (await guard(() => onCreate(newName.trim(), null))) setNewName("");
  };

  const startEditing = (folder: FolderView) => {
    setEditing(folder.id);
    setDraftName(folder.name);
    setConfirmingRemove(null);
  };

  const saveName = async (id: string) => {
    if (await guard(() => onRename(id, draftName))) setEditing(null);
  };

  return (
    <div className="pane pane--top">
      <div>
        <h1 className="pane__title">Folders</h1>
        <p className="pane__hint">
          Group accounts by client, by company, however you work.
        </p>
      </div>

      <div className="pane__form">
        <div className="row-inline">
          <input
            className="field"
            placeholder="New folder name"
            aria-label="New folder name"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void create();
            }}
          />
          <button
            className="button button--primary"
            type="button"
            onClick={() => void create()}
            disabled={busy || !newName.trim()}
          >
            Add
          </button>
        </div>

        {error && <p className="error">{error}</p>}

        {folders.length === 0 ? (
          <p className="pane__hint">No folders yet.</p>
        ) : (
          <ul className="folder-list">
            {folders.map((folder) => (
              <li key={folder.id} style={{ paddingLeft: folder.depth * 16 }}>
                {editing === folder.id ? (
                  <div className="folder-edit">
                    <div className="row-inline">
                      <input
                        className="field"
                        autoFocus
                        aria-label="Folder name"
                        value={draftName}
                        onChange={(e) => setDraftName(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") void saveName(folder.id);
                          if (e.key === "Escape") setEditing(null);
                        }}
                      />
                      <button
                        className="button button--primary"
                        type="button"
                        onClick={() => void saveName(folder.id)}
                        disabled={busy}
                      >
                        Save
                      </button>
                    </div>

                    <div className="icon-grid" role="group" aria-label="Folder icon">
                      {FOLDER_ICONS.map((icon) => (
                        <button
                          key={icon}
                          type="button"
                          className={`icon-choice${
                            folder.icon === icon ? " icon-choice--on" : ""
                          }`}
                          aria-label={`Use ${icon}`}
                          aria-pressed={folder.icon === icon}
                          onClick={() =>
                            void guard(() =>
                              onSetIcon(folder.id, folder.icon === icon ? null : icon),
                            )
                          }
                        >
                          {icon}
                        </button>
                      ))}
                    </div>

                    <select
                      className="field"
                      aria-label="Parent folder"
                      value={folder.parentId ?? ""}
                      onChange={(e) =>
                        void guard(() => onMove(folder.id, e.target.value || null))
                      }
                    >
                      <option value="">At the top level</option>
                      {folders
                        .filter((f) => f.id !== folder.id)
                        .map((f) => (
                          <option key={f.id} value={f.id}>
                            {indented(f)}
                          </option>
                        ))}
                    </select>

                    {confirmingRemove === folder.id ? (
                      <>
                        <p className="warning">
                          Accounts in this folder will move out of it. Nothing is
                          deleted.
                        </p>
                        <div className="button-row">
                          <button
                            className="button button--quiet"
                            type="button"
                            onClick={() => setConfirmingRemove(null)}
                          >
                            Keep it
                          </button>
                          <button
                            className="button button--danger"
                            type="button"
                            disabled={busy}
                            onClick={() =>
                              void guard(() => onRemove(folder.id)).then(() => {
                                setConfirmingRemove(null);
                                setEditing(null);
                              })
                            }
                          >
                            Delete folder
                          </button>
                        </div>
                      </>
                    ) : (
                      <div className="button-row">
                        <button
                          className="button button--danger button-row__spacer"
                          type="button"
                          onClick={() => setConfirmingRemove(folder.id)}
                        >
                          Delete
                        </button>
                        <button
                          className="button button--quiet"
                          type="button"
                          onClick={() => setEditing(null)}
                        >
                          Done
                        </button>
                      </div>
                    )}
                  </div>
                ) : (
                  <button
                    className="folder-row"
                    type="button"
                    onClick={() => startEditing(folder)}
                  >
                    <span className="folder-row__icon">{folder.icon ?? "📁"}</span>
                    <span className="folder-row__name">{folder.name}</span>
                    <span className="folder-row__count">{folder.accountCount}</span>
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="button-row">
        <button className="button button--quiet" type="button" onClick={onDone}>
          Done
        </button>
      </div>
    </div>
  );
}
