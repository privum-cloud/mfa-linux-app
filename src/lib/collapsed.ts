/**
 * Which folders the user has collapsed.
 *
 * This lived inside the account list, which meant it was thrown away every time
 * another screen opened — collapse a folder, edit an account, and everything
 * was expanded again.
 *
 * It belongs in local storage rather than in the vault. It is presentation, not
 * data: it should not travel between machines, should not count as a change
 * worth merging, and should not make the vault rewrite itself every time a
 * triangle is clicked. The identifiers kept here are random UUIDs that say
 * nothing without the vault they came from.
 */

const KEY = "tessera.collapsedFolders";

/**
 * The stored set, or `null` when nothing has ever been stored.
 *
 * The difference matters: nothing stored means a first run, where every folder
 * starts collapsed. An empty set means the user expanded everything, and
 * undoing that on the next launch would be maddening.
 */
export function readCollapsed(): Set<string> | null {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (raw === null) return null;

    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return null;
    return new Set(parsed.filter((id): id is string => typeof id === "string"));
  } catch {
    // Storage disabled, or a value someone else wrote. Losing the collapse
    // state is not worth an error the user has to read.
    return null;
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
    // Same reasoning: a convenience, not the user's data.
  }
}
