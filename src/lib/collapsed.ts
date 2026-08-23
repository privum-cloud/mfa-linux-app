/**
 * Which folders the user has collapsed.
 *
 * This lived inside the account list, which meant it was thrown away every time
 * the user opened another screen and came back — collapse a folder, edit an
 * account, and everything was expanded again.
 *
 * It belongs in local storage rather than in the vault. It is presentation, not
 * data: it should not travel between machines, should not count as a change
 * worth merging, and should not make the vault rewrite itself every time a
 * triangle is clicked. The identifiers stored here are random UUIDs that say
 * nothing without the vault they came from.
 */

const KEY = "tessera.collapsedFolders";

export function readCollapsed(): Set<string> {
  try {
    const raw = window.localStorage.getItem(KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((id): id is string => typeof id === "string"));
  } catch {
    // A browser with storage disabled, or a value someone else wrote. Losing
    // the collapse state is not worth an error the user has to read.
    return new Set();
  }
}

/**
 * Save the collapsed set, dropping folders that no longer exist so the list
 * does not grow forever behind the user's back.
 */
export function writeCollapsed(collapsed: Set<string>, knownIds: string[]): void {
  try {
    const known = new Set(knownIds);
    const kept = [...collapsed].filter((id) => known.has(id));
    window.localStorage.setItem(KEY, JSON.stringify(kept));
  } catch {
    // Same reasoning: this is a convenience, not the user's data.
  }
}
