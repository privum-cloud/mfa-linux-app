import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import Spinner from "../components/Spinner";
import * as api from "../lib/api";
import type { ImportSummary } from "../lib/api";

interface Props {
  error: string | null;
  onImported: () => void;
  onError: (message: string) => void;
  onDone: () => void;
}

type Mode = "import" | "export";

export default function ImportExport({
  error,
  onImported,
  onError,
  onDone,
}: Props) {
  const [mode, setMode] = useState<Mode>("import");
  const [uri, setUri] = useState("");
  const [summary, setSummary] = useState<ImportSummary | null>(null);
  const [codes, setCodes] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);

  const report = (result: ImportSummary) => {
    setSummary(result);
    onImported();
  };

  const pickFile = async () => {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg"] }],
    });
    if (typeof picked !== "string") return;

    setBusy(true);
    try {
      report(await api.importFromImage(picked));
    } catch (e: unknown) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const pasteLink = async () => {
    setBusy(true);
    try {
      report(await api.importFromMigrationUri(uri.trim()));
      setUri("");
    } catch (e: unknown) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const showExport = async () => {
    setBusy(true);
    try {
      setCodes(await api.exportMigrationQrs());
    } catch (e: unknown) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const switchTo = (next: Mode) => {
    setMode(next);
    setSummary(null);
    setCodes(null);
    if (next === "export") void showExport();
  };

  return (
    <div className="pane pane--top">
      <div>
        <h1 className="pane__title">
          {mode === "import" ? "Import from Google Authenticator" : "Send to a phone"}
        </h1>
        <p className="pane__hint">
          {mode === "import"
            ? "On your phone, open Google Authenticator, choose Transfer accounts, then Export. Screenshot the QR code it shows and open that file here."
            : "Open Google Authenticator on your phone, choose Import accounts, and scan these."}
        </p>
      </div>

      {mode === "import" ? (
        <div className="pane__form">
          <button
            className={`button button--primary${busy ? " button--busy" : ""}`}
            type="button"
            onClick={() => void pickFile()}
            disabled={busy}
          >
            {busy ? (
              <span className="button__busy">
                <Spinner />
                Reading the image…
              </span>
            ) : (
              "Choose an image"
            )}
          </button>

          <p className="pane__hint">Or paste the link behind the QR code:</p>
          <input
            className="field field--mono"
            placeholder="otpauth-migration://offline?data=..."
            aria-label="Migration link"
            value={uri}
            onChange={(e) => setUri(e.target.value)}
          />
          <button
            className={`button button--quiet${busy ? " button--busy" : ""}`}
            type="button"
            onClick={() => void pasteLink()}
            disabled={busy || uri.trim().length === 0}
          >
            {busy ? (
              <span className="button__busy">
                <Spinner />
                Importing…
              </span>
            ) : (
              "Import the link"
            )}
          </button>

          {summary && (
            <p className="notice">
              {summary.added === 0
                ? "Nothing new — Tessera already had every account in that export."
                : `Added ${summary.added} account${summary.added === 1 ? "" : "s"}.`}
              {summary.alreadyPresent > 0 && summary.added > 0
                ? ` ${summary.alreadyPresent} were already here.`
                : ""}
            </p>
          )}
        </div>
      ) : (
        <div className="pane__form">
          <p className="warning">
            These codes carry every secret in your vault. Show them only to your
            own phone.
          </p>
          {codes?.map((src, index) => (
            <figure className="qr" key={src.slice(0, 48)}>
              <img src={src} alt={`Export code ${index + 1}`} />
              <figcaption>
                {codes.length > 1
                  ? `Scan ${index + 1} of ${codes.length}`
                  : "Scan this"}
              </figcaption>
            </figure>
          ))}
        </div>
      )}

      {error && <p className="error">{error}</p>}

      <div className="button-row">
        <button
          className="button button--quiet button-row__spacer"
          type="button"
          onClick={() => switchTo(mode === "import" ? "export" : "import")}
        >
          {mode === "import" ? "Send to a phone instead" : "Import instead"}
        </button>
        <button className="button button--quiet" type="button" onClick={onDone}>
          Done
        </button>
      </div>
    </div>
  );
}
