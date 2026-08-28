import { openUrl } from "@tauri-apps/plugin-opener";

import type { UpdateDelivery } from "../lib/api";
import type { UpdatePhase } from "../lib/useUpdate";

interface Props {
  phase: UpdatePhase;
  delivery: UpdateDelivery;
  releasesUrl: string;
  onInstall: () => void;
  onDismiss: () => void;
}

export default function UpdateBanner({
  phase,
  delivery,
  releasesUrl,
  onInstall,
  onDismiss,
}: Props) {
  if (phase.name === "quiet") return null;

  const openReleases = () => {
    void openUrl(releasesUrl);
  };

  if (phase.name === "installing") {
    return (
      <div className="update" role="status">
        <span className="update__text">
          {phase.percent === null
            ? "Downloading…"
            : `Downloading… ${phase.percent}%`}
        </span>
      </div>
    );
  }

  if (phase.name === "installed") {
    return (
      <div className="update" role="status">
        <span className="update__text">Installed. Restarting…</span>
      </div>
    );
  }

  if (phase.name === "failed") {
    return (
      <div className="update update--failed" role="alert">
        <span className="update__text">
          The update could not be installed. {phase.message}
        </span>
        <button className="button button--quiet" type="button" onClick={openReleases}>
          Download it
        </button>
        <button
          className="button button--quiet"
          type="button"
          onClick={onDismiss}
          aria-label="Dismiss"
        >
          Not now
        </button>
      </div>
    );
  }

  return (
    <div className="update" role="status">
      <span className="update__text">
        Tessera {phase.version} is out.
        {delivery === "needs_admin" && (
          // Said before the click, not after. Someone surprised by a root
          // password prompt learns to type root passwords into surprises.
          <>
            {" "}
            Your system will ask for an administrator password, because your
            package manager owns this installation.
          </>
        )}
      </span>
      <button className="button button--primary" type="button" onClick={onInstall}>
        Update
      </button>
      <button
        className="button button--quiet"
        type="button"
        onClick={onDismiss}
        aria-label="Dismiss"
      >
        Not now
      </button>
    </div>
  );
}
