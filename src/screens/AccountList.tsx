import { useEffect, useMemo, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

import CountdownRing from "../components/CountdownRing";
import FolderIcon from "../components/FolderIcon";
import { readCollapsed, writeCollapsed } from "../lib/collapsed";
import type { AccountView, FolderView } from "../lib/api";

interface Props {
  accounts: AccountView[];
  folders: FolderView[];
  clipboardClearSecs: number;
  onEdit: (account: AccountView) => void;
  onActivity: () => void;
}

export default function AccountList({
  accounts,
  folders,
  clipboardClearSecs,
  onEdit,
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

  /** Before the first run settles, treat everything as collapsed — rendering it
   *  open for one frame would show a flash of the very thing we are avoiding. */
  const isCollapsed = (id: string) => (collapsed === null ? true : collapsed.has(id));

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
    <li key={account.id} style={indent ? { paddingLeft: indent } : undefined}>
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
        <ul className="rows">{matches.map((a) => row(a, 0, true))}</ul>
      ) : (
        <ul className="rows">
          {folders
            .filter((f) => !hiddenByAncestor.has(f.id))
            .map((folder) => {
              const inside = matches.filter((a) => a.folderId === folder.id);
              const folderCollapsed = isCollapsed(folder.id);
              return (
                <li key={folder.id} className="section">
                  <button
                    className="section__header"
                    type="button"
                    style={{ paddingLeft: 12 + folder.depth * 14 }}
                    onClick={() => toggle(folder.id)}
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
                  {!folderCollapsed && (
                    <ul className="rows rows--nested">
                      {inside.map((a) => row(a, folder.depth * 14))}
                    </ul>
                  )}
                </li>
              );
            })}

          {loose.length > 0 && folders.length > 0 && (
            <li className="section">
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

      {toast && <div className="toast">{toast}</div>}
    </>
  );
}
