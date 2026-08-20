import { invoke } from "@tauri-apps/api/core";

export type AccountKind = "totp" | "hotp" | "steam";

/**
 * One row, as the core renders it. There is deliberately no secret here: the
 * interface has no use for one and no way to hold it safely.
 */
export interface AccountView {
  id: string;
  issuer: string;
  label: string;
  group: string | null;
  kind: AccountKind;
  code: string;
  /** Zero for HOTP, which does not expire. */
  secondsRemaining: number;
  period: number;
}

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
}

export interface Settings {
  idleTimeoutSecs: number;
  clipboardClearSecs: number;
}

interface RawAccountView {
  id: string;
  issuer: string;
  label: string;
  group: string | null;
  kind: AccountKind;
  code: string;
  seconds_remaining: number;
  period: number;
}

interface RawSettings {
  idle_timeout_secs: number;
  clipboard_clear_secs: number;
}

const toAccount = (raw: RawAccountView): AccountView => ({
  id: raw.id,
  issuer: raw.issuer,
  label: raw.label,
  group: raw.group,
  kind: raw.kind,
  code: raw.code,
  secondsRemaining: raw.seconds_remaining,
  period: raw.period,
});

const toSettings = (raw: RawSettings): Settings => ({
  idleTimeoutSecs: raw.idle_timeout_secs,
  clipboardClearSecs: raw.clipboard_clear_secs,
});

export const vaultStatus = () => invoke<VaultStatus>("vault_status");
export const createVault = (password: string) =>
  invoke<void>("create_vault", { password });
export const unlockVault = (password: string) =>
  invoke<void>("unlock_vault", { password });
export const lockVault = () => invoke<void>("lock_vault");

export const listAccounts = async (): Promise<AccountView[]> =>
  (await invoke<RawAccountView[]>("list_accounts")).map(toAccount);

export const addAccountFromUri = (uri: string) =>
  invoke<void>("add_account_from_uri", { uri });

/** A type alias, not an interface: `invoke` takes Record<string, unknown>, and
 *  TypeScript grants an implicit index signature to aliases but not interfaces. */
export type ManualAccount = {
  issuer: string;
  label: string;
  secret: string;
  kind: AccountKind;
  algorithm: "SHA1" | "SHA256" | "SHA512";
  digits: number;
  period: number;
};

export const addAccountManual = (account: ManualAccount) =>
  invoke<void>("add_account_manual", { ...account });

export const updateAccount = (
  id: string,
  issuer: string,
  label: string,
  group: string | null,
) => invoke<void>("update_account", { id, issuer, label, group });

export const deleteAccount = (id: string) =>
  invoke<void>("delete_account", { id });

export const pollIdleLock = () => invoke<boolean>("poll_idle_lock");
export const noteActivity = () => invoke<void>("note_activity");

export const getSettings = async (): Promise<Settings> =>
  toSettings(await invoke<RawSettings>("get_settings"));

export const setSettings = async (settings: Settings): Promise<Settings> =>
  toSettings(
    await invoke<RawSettings>("set_settings", {
      settings: {
        idle_timeout_secs: settings.idleTimeoutSecs,
        clipboard_clear_secs: settings.clipboardClearSecs,
      },
    }),
  );

export interface ImportSummary {
  added: number;
  alreadyPresent: number;
}

const toSummary = (raw: { added: number; already_present: number }): ImportSummary => ({
  added: raw.added,
  alreadyPresent: raw.already_present,
});

export const importFromImage = async (path: string): Promise<ImportSummary> =>
  toSummary(
    await invoke<{ added: number; already_present: number }>(
      "import_from_image",
      { path },
    ),
  );

export const importFromMigrationUri = async (
  uri: string,
): Promise<ImportSummary> =>
  toSummary(
    await invoke<{ added: number; already_present: number }>(
      "import_from_migration_uri",
      { uri },
    ),
  );

/** PNG data URLs. The payload never crosses as text — it is rendered in Rust. */
export const exportMigrationQrs = () => invoke<string[]>("export_migration_qrs");
