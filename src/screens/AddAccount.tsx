import { useState, type FormEvent } from "react";

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
  const submit = async (event: FormEvent) => {
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
    mode === "link"
      ? uri.trim().length > 0
      : secret.trim().length > 0 && label.trim().length > 0;

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

      <form className="pane__form" onSubmit={(e) => void submit(e)}>
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
            className="button button--quiet button-row__spacer"
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
