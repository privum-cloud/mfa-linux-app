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
      void writeText("").catch(() => {});
    }, clipboardClearSecs * 1000);
  };

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
