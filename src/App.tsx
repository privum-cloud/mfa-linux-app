import { useEffect, useState } from "react";
import { previewCode, type CodeView } from "./lib/api";

/** The RFC 4226 test secret, so the smoke screen has something to show. */
const SAMPLE_SECRET = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

export default function App() {
  const [view, setView] = useState<CodeView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const tick = () =>
      previewCode({
        secret: SAMPLE_SECRET,
        kind: "totp",
        algorithm: "SHA1",
        digits: 6,
        period: 30,
        counter: 0,
      })
        .then(setView)
        .catch((e) => setError(String(e)));

    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, []);

  return (
    <main className="shell">
      {error ? (
        <p role="alert">{error}</p>
      ) : (
        <div>
          <p className="code">{view?.code ?? "······"}</p>
          <p className="countdown">{view?.secondsRemaining ?? 0}s</p>
        </div>
      )}
    </main>
  );
}
