import { useEffect, useMemo, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

import CountdownRing from "../components/CountdownRing";
import DragHandle from "../components/DragHandle";
import FolderIcon, {
  FOLDER_ICONS,
  FOLDER_ICON_LABELS,
} from "../components/FolderIcon";
import FolderMenu from "../components/FolderMenu";
import { readCollapsed, writeCollapsed } from "../lib/collapsed";
import { useDragToFolder } from "../lib/useDragToFolder";
import type { AccountView, FolderView } from "../lib/api";

interface Props {
  accounts: AccountView[];
  folders: FolderView[];
  clipboardClearSecs: number;
  onEdit: (account: AccountView) => void;
  onMoveToFolder: (id: string, folderId: string | null) => void;
  onCreateFolder: (name: string, parentId: string | null) => void;
  onRenameFolder: (id: string, name: string) => void;
  onSetFolderIcon: (id: string, icon: string | null) => void;
  onRemoveFolder: (id: string) => void;
  onActivity: () => void;
}

export default function AccountList({
  accounts,
  folders,
  clipboardClearSecs,
  onEdit,
  onMoveToFolder,
  onCreateFolder,
  onRenameFolder,
  onSetFolderIcon,
  onRemoveFolder,
  onActivity,
}: Props) {
  const [query, setQuery] = useState("");
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  // Read back rather than start empty: this component is unmounted every time
  // the user opens another screen, and starting empty expanded every folder
  // the moment they edited an account.
  //
  // `null` means nothing has ever been stored — a first run, where every folder
  // starts collapsed. It is deliberately not the same as an empty set, which
  // means the user expanded everything and would not thank us for undoing it.
  const [collapsed, setCollapsed] = useState<Set<string> | null>(readCollapsed);

  // Once the folders arrive on a first run, collapse them and remember it.
  useEffect(() => {
    if (collapsed === null && folders.length > 0) {
      setCollapsed(new Set(folders.map((f) => f.id)));
    }
  }, [collapsed, folders]);

  useEffect(() => {
    if (collapsed !== null && folders.length > 0) {
      writeCollapsed(
        collapsed,
        folders.map((f) => f.id),
      );
    }
  }, [collapsed, folders]);

  // While a drag is in flight the tree is folded down to its folder headings,
  // so every destination is on screen at once instead of somewhere below the
  // fold. It overlays the real set rather than replacing it: nothing here is
  // written to storage, and letting go restores exactly what was open before.
  const [foldedForDrag, setFoldedForDrag] = useState<Set<string> | null>(null);
  // Right-click opens this; picking from it opens the panel below.
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(
    null,
  );
  // The four actions all need somewhere to happen, and they all happen in the
  // same strip under the folder's own heading rather than in four places.
  const [pending, setPending] = useState<
    { id: string; kind: "add" | "rename" | "icon" | "remove" } | null
  >(null);
  const [draft, setDraft] = useState("");

  /** Before the first run settles, treat everything as collapsed — rendering it
   *  open for one frame would show a flash of the very thing we are avoiding. */
  const isCollapsed = (id: string) =>
    foldedForDrag !== null
      ? foldedForDrag.has(id)
      : collapsed === null
        ? true
        : collapsed.has(id);

  const needle = query.trim().toLowerCase();
  const searching = needle.length > 0;

  const folderName = useMemo(() => {
    const byId = new Map(folders.map((f) => [f.id, f.name]));
    return (id: string | null) => (id ? (byId.get(id) ?? null) : null);
  }, [folders]);

  const matches = useMemo(() => {
    if (!searching) return accounts;
    return accounts.filter((a) =>
      `${a.issuer} ${a.label} ${folderName(a.folderId) ?? ""}`
        .toLowerCase()
        .includes(needle),
    );
  }, [accounts, needle, searching, folderName]);

  /** A folder is hidden when any of its ancestors is collapsed. */
  const hiddenByAncestor = useMemo(() => {
    const parentOf = new Map(folders.map((f) => [f.id, f.parentId]));
    const hidden = new Set<string>();
    for (const folder of folders) {
      let at = folder.parentId;
      let guard = folders.length;
      while (at && guard-- > 0) {
        if (isCollapsed(at)) {
          hidden.add(folder.id);
          break;
        }
        at = parentOf.get(at) ?? null;
      }
    }
    return hidden;
  }, [folders, collapsed]);

  /** Open a folder, leaving an already-open one alone. Distinct from `toggle`
   *  because a folder rested on mid-drag must open, never close. */
  const expand = (id: string) => {
    // Mid-drag the spring-open works on the temporary set, so opening a folder
    // to reach a subfolder does not survive the drag as a real preference.
    if (foldedForDrag !== null) {
      setFoldedForDrag((previous) => {
        if (previous === null || !previous.has(id)) return previous;
        const next = new Set(previous);
        next.delete(id);
        return next;
      });
      return;
    }
    setCollapsed((previous) => {
      if (previous === null) {
        return new Set(folders.map((f) => f.id).filter((f) => f !== id));
      }
      if (!previous.has(id)) return previous;
      const next = new Set(previous);
      next.delete(id);
      return next;
    });
  };

  const closePanel = () => {
    setPending(null);
    setDraft("");
  };

  const pickFromMenu = (id: string, kind: NonNullable<typeof pending>["kind"]) => {
    onActivity();
    setMenu(null);
    // Opened first: acting on a folder you cannot see is acting somewhere you
    // are guessing at.
    expand(id);
    setPending({ id, kind });
    setDraft(kind === "rename" ? (folders.find((f) => f.id === id)?.name ?? "") : "");
  };

  const commitPanel = () => {
    if (pending === null) return;
    const name = draft.trim();
    if (pending.kind === "add") {
      if (!name) return;
      onCreateFolder(name, pending.id);
    } else if (pending.kind === "rename") {
      if (!name) return;
      onRenameFolder(pending.id, name);
    }
    closePanel();
  };

  const toggle = (id: string) => {
    onActivity();
    setCollapsed((previous) => {
      // A null previous means the first run has not settled; everything is
      // collapsed, so toggling one starts from all of them.
      const next = new Set(previous ?? folders.map((f) => f.id));
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const drag = useDragToFolder({
    onMove: onMoveToFolder,
    onSpringOpen: expand,
    isCollapsed,
    onBegin: (from) => {
      // Everything folds except the chain the dragged row is sitting in. Fold
      // its own folder and the row goes off the page; fold an ancestor and it
      // goes with it, and either way the browser drops the drag it was in the
      // middle of.
      const parentOf = new Map(folders.map((f) => [f.id, f.parentId]));
      const keepOpen = new Set<string>();
      let at = from;
      let guard = folders.length;
      while (at !== null && guard-- > 0) {
        keepOpen.add(at);
        at = parentOf.get(at) ?? null;
      }
      setFoldedForDrag(
        new Set(folders.map((f) => f.id).filter((id) => !keepOpen.has(id))),
      );
    },
    onFinish: () => setFoldedForDrag(null),
    onActivity,
  });

  // Dragging needs somewhere to drop. Searching flattens the tree and no
  // folder is on screen; with no folders at all there is nothing to aim at.
  const canDrag = !searching && folders.length > 0;
  // Keyed on having folders rather than on canDrag, so the rows do not shift
  // sideways the moment someone types in the search box.
  const gripped = folders.length > 0 ? " rows--gripped" : "";

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
      void writeText("").catch(() => {});
    }, clipboardClearSecs * 1000);
  };

  const row = (account: AccountView, indent: number, withFolder = false) => (
    <li
      key={account.id}
      className={drag.draggingId === account.id ? "row-item--lifted" : undefined}
      style={indent ? { paddingLeft: indent } : undefined}
    >
      {canDrag && (
        <span
          className="grip-slot"
          aria-hidden="true"
          style={indent ? { left: 8 + indent } : undefined}
          {...drag.handleProps(account.id, account.folderId)}
        >
          <DragHandle />
        </span>
      )}

      <button
        className="row"
        type="button"
        onClick={() => void copy(account)}
        title="Copy code"
      >
        <span className="row__identity">
          <span className="row__issuer">{account.issuer || account.label}</span>
          <span className="row__label">
            {account.issuer ? account.label : ""}
            {withFolder && folderName(account.folderId) ? (
              <span className="row__folder"> · {folderName(account.folderId)}</span>
            ) : null}
          </span>
        </span>

        <span
          className={`row__code${copiedId === account.id ? " row__code--copied" : ""}`}
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
      >
        ⋯
      </button>
    </li>
  );

  if (accounts.length === 0) {
    return (
      <div className="empty">
        <p>No accounts yet.</p>
        <p>
          Add one by pasting the otpauth:// link behind a service&apos;s QR code,
          or by typing the secret it gave you.
        </p>
      </div>
    );
  }

  const loose = matches.filter((a) => !a.folderId);

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
      ) : searching ? (
        /* Searching means find this now, not browse: the tree flattens and each
           row says which folder it came from. */
        <ul className={`rows${gripped}`}>
          {matches.map((a) => row(a, 0, true))}
        </ul>
      ) : (
        <ul className={`rows${gripped}`}>
          {folders
            .filter((f) => !hiddenByAncestor.has(f.id))
            .map((folder) => {
              const inside = matches.filter((a) => a.folderId === folder.id);
              const folderCollapsed = isCollapsed(folder.id);
              return (
                <li
                  key={folder.id}
                  className={`section${
                    drag.isOver(folder.id) ? " section--drop" : ""
                  }`}
                  {...drag.targetProps(folder.id)}
                >
                  <button
                    className="section__header"
                    type="button"
                    style={{ paddingLeft: 12 + folder.depth * 14 }}
                    onClick={() => toggle(folder.id)}
                    onContextMenu={(event) => {
                      // Without this the WebView2 menu opens on top of ours.
                      event.preventDefault();
                      onActivity();
                      setMenu({
                        id: folder.id,
                        x: event.clientX,
                        y: event.clientY,
                      });
                    }}
                    aria-expanded={!folderCollapsed}
                  >
                    <span className="section__twisty">
                      {folderCollapsed ? "▸" : "▾"}
                    </span>
                    <span className="section__icon">
                      <FolderIcon icon={folder.icon} size={14} />
                    </span>
                    <span className="section__name">{folder.name}</span>
                    <span className="section__count">{folder.accountCount}</span>
                  </button>
                  {pending?.id === folder.id && (
                    <div
                      className="section__panel"
                      style={{ paddingLeft: 26 + folder.depth * 14 }}
                    >
                      {(pending.kind === "add" || pending.kind === "rename") && (
                        <input
                          className="field field--compact"
                          autoFocus
                          placeholder={
                            pending.kind === "add" ? "Name it" : folder.name
                          }
                          aria-label={
                            pending.kind === "add"
                              ? `Name of the new subfolder in ${folder.name}`
                              : `New name for ${folder.name}`
                          }
                          value={draft}
                          onChange={(e) => setDraft(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") commitPanel();
                            if (e.key === "Escape") closePanel();
                          }}
                          onBlur={closePanel}
                        />
                      )}

                      {pending.kind === "icon" && (
                        <div
                          className="icon-grid"
                          role="group"
                          aria-label={`Icon for ${folder.name}`}
                        >
                          {FOLDER_ICONS.map((icon) => (
                            <button
                              key={icon}
                              className={`icon-choice${
                                folder.icon === icon ? " icon-choice--on" : ""
                              }`}
                              type="button"
                              aria-label={FOLDER_ICON_LABELS[icon] ?? icon}
                              title={FOLDER_ICON_LABELS[icon] ?? icon}
                              onClick={() => {
                                onSetFolderIcon(
                                  folder.id,
                                  folder.icon === icon ? null : icon,
                                );
                                closePanel();
                              }}
                            >
                              <FolderIcon icon={icon} size={17} />
                            </button>
                          ))}
                        </div>
                      )}

                      {pending.kind === "remove" && (
                        <div className="section__confirm">
                          <p className="pane__hint">
                            Delete “{folder.name}”? Everything inside moves up
                            to where the folder was; no account is lost.
                          </p>
                          <div className="button-row">
                            <button
                              className="button button--danger button-row__spacer"
                              type="button"
                              onClick={() => {
                                onRemoveFolder(folder.id);
                                closePanel();
                              }}
                            >
                              Delete folder
                            </button>
                            <button
                              className="button button--quiet"
                              type="button"
                              onClick={closePanel}
                            >
                              Cancel
                            </button>
                          </div>
                        </div>
                      )}
                    </div>
                  )}

                  {!folderCollapsed && (
                    <ul className="rows rows--nested">
                      {inside.map((a) => row(a, folder.depth * 14))}
                    </ul>
                  )}
                </li>
              );
            })}

          {(loose.length > 0 || drag.active) && folders.length > 0 && (
            <li
              className={`section${drag.isOver(null) ? " section--drop" : ""}`}
              {...drag.targetProps(null)}
            >
              <div className="section__header section__header--plain">
                <span className="section__name">Ungrouped</span>
                <span className="section__count">{loose.length}</span>
              </div>
              <ul className="rows rows--nested">{loose.map((a) => row(a, 0))}</ul>
            </li>
          )}

          {folders.length === 0 && loose.map((a) => row(a, 0))}
        </ul>
      )}

      {menu && (
        <FolderMenu
          x={menu.x}
          y={menu.y}
          folderName={folders.find((f) => f.id === menu.id)?.name ?? ""}
          onPick={(kind) => pickFromMenu(menu.id, kind)}
          onClose={() => setMenu(null)}
        />
      )}

      {toast && <div className="toast">{toast}</div>}
    </>
  );
}
