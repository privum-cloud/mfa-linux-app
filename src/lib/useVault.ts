import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "./api";
import type {
  AccountView,
  FolderView,
  ManualAccount,
  Settings,
  VaultStatus,
} from "./api";

/**
 * Application state: whether the vault is open, and what is in it.
 *
 * Codes are refreshed by a single one-second tick rather than one timer per
 * row. Twenty rows means twenty timers drifting apart, and the countdowns stop
 * agreeing with each other.
 */
export function useVault() {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [accounts, setAccounts] = useState<AccountView[]>([]);
  const [folders, setFolders] = useState<FolderView[]>([]);
  const [settings, setSettingsState] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  const unlocked = status?.unlocked ?? false;
  const unlockedRef = useRef(unlocked);
  unlockedRef.current = unlocked;

  const refreshStatus = useCallback(async () => {
    setStatus(await api.vaultStatus());
  }, []);

  const refreshAccounts = useCallback(async () => {
    // Folders come along on every refresh: a rename or a move has to show up in
    // the same paint as the accounts it reorganises, or the list flickers
    // through an inconsistent state.
    const [rows, tree] = await Promise.all([api.listAccounts(), api.listFolders()]);
    setAccounts(rows);
    setFolders(tree);
  }, []);

  useEffect(() => {
    refreshStatus().catch((e: unknown) => setError(String(e)));
  }, [refreshStatus]);

  // One tick drives every countdown, and doubles as the idle-lock check.
  useEffect(() => {
    if (!unlocked) return;

    let cancelled = false;
    const tick = async () => {
      try {
        if (await api.pollIdleLock()) {
          if (!cancelled) {
            setAccounts([]);
            await refreshStatus();
          }
          return;
        }
        // Take in anything another machine wrote before rendering, so a
        // shared vault converges without the user doing anything.
        await api.refreshVault().catch(() => {
          // A failed refresh only means this tick shows slightly stale rows.
        });
        const [rows, tree] = await Promise.all([
          api.listAccounts(),
          api.listFolders(),
        ]);
        if (!cancelled) {
          setAccounts(rows);
          setFolders(tree);
        }
      } catch (e: unknown) {
        if (!cancelled) setError(String(e));
      }
    };

    void tick();
    const timer = window.setInterval(() => void tick(), 1000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [unlocked, refreshStatus]);

  useEffect(() => {
    if (!unlocked) {
      setSettingsState(null);
      return;
    }
    api
      .getSettings()
      .then(setSettingsState)
      .catch((e: unknown) => setError(String(e)));
  }, [unlocked]);

  /** Run a command, surface its message on failure, and refresh what changed. */
  const run = useCallback(
    async (action: () => Promise<unknown>, refresh = true) => {
      setError(null);
      try {
        await action();
        if (refresh && unlockedRef.current) await refreshAccounts();
        return true;
      } catch (e: unknown) {
        setError(String(e));
        return false;
      }
    },
    [refreshAccounts],
  );

  const actions = {
    create: async (password: string) => {
      const ok = await run(() => api.createVault(password), false);
      if (ok) await refreshStatus();
      return ok;
    },
    unlock: async (password: string) => {
      const ok = await run(() => api.unlockVault(password), false);
      if (ok) await refreshStatus();
      return ok;
    },
    lock: async () => {
      await run(() => api.lockVault(), false);
      setAccounts([]);
      setFolders([]);
      await refreshStatus();
    },
    addFromUri: (uri: string) => run(() => api.addAccountFromUri(uri)),
    addManual: (account: ManualAccount) =>
      run(() => api.addAccountManual(account)),
    update: (id: string, issuer: string, label: string, group: string | null) =>
      run(() => api.updateAccount(id, issuer, label, group)),
    remove: (id: string) => run(() => api.deleteAccount(id)),
    createFolder: (name: string, parentId: string | null) =>
      run(() => api.createFolder(name, parentId)),
    renameFolder: (id: string, name: string) => run(() => api.renameFolder(id, name)),
    setFolderIcon: (id: string, icon: string | null) =>
      run(() => api.setFolderIcon(id, icon)),
    moveFolder: (id: string, parentId: string | null) =>
      run(() => api.moveFolder(id, parentId)),
    removeFolder: (id: string) => run(() => api.removeFolder(id)),
    moveAccountToFolder: (id: string, folderId: string | null) =>
      run(() => api.moveAccountToFolder(id, folderId)),
    saveSettings: (next: Settings) =>
      run(async () => {
        setSettingsState(await api.setSettings(next));
      }, false),
    noteActivity: () => {
      void api.noteActivity().catch(() => {
        // Losing one activity ping only means the idle timer runs a little
        // early. Not worth interrupting the user over.
      });
    },
    refresh: () => {
      void refreshAccounts().catch((e: unknown) => setError(String(e)));
    },
    setError: (message: string) => setError(message),
    setVaultLocation: async (folder: string) => {
      const ok = await run(() => api.setVaultLocation(folder), false);
      if (ok) {
        setAccounts([]);
        setFolders([]);
        await refreshStatus();
      }
      return ok;
    },
    clearError: () => setError(null),
  };

  return { status, accounts, folders, settings, error, actions };
}
