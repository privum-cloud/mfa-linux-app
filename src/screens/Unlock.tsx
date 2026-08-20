import { useState, type FormEvent } from "react";

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

  const tooShort =
    !existing && password.length > 0 && password.length < MIN_LENGTH;
  const mismatched =
    !existing && confirmation.length > 0 && confirmation !== password;
  const ready = existing
    ? password.length > 0
    : password.length >= MIN_LENGTH && confirmation === password;

  const submit = async (event: FormEvent) => {
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

      <form className="pane__form" onSubmit={(e) => void submit(e)}>
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
