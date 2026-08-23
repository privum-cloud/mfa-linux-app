import { useState } from "react";

import { useVault } from "./lib/useVault";
import type { AccountView } from "./lib/api";
import AccountList from "./screens/AccountList";
import AddAccount from "./screens/AddAccount";
import EditAccount from "./screens/EditAccount";
import FolderIcon from "./components/FolderIcon";
import FolderEditor from "./screens/FolderEditor";
import SettingsScreen from "./screens/SettingsScreen";
import ImportExport from "./screens/ImportExport";
import Unlock from "./screens/Unlock";

type Screen =
  | { name: "list" }
  | { name: "add" }
  | { name: "edit"; account: AccountView }
  | { name: "settings" }
  | { name: "folders" }
  | { name: "transfer" };

export default function App() {
  const { status, accounts, folders, settings, error, actions } = useVault();
  const [screen, setScreen] = useState<Screen>({ name: "list" });

  // The very first render, before the core has answered.
  if (!status) return <main className="shell" />;

  if (!status.unlocked) {
    return (
      <main className="shell">
        <Unlock
          existing={status.exists}
          error={error}
          onSubmit={status.exists ? actions.unlock : actions.create}
        />
      </main>
    );
  }

  const back = () => {
    actions.clearError();
    setScreen({ name: "list" });
  };

  if (screen.name === "add") {
    return (
      <main className="shell">
        <AddAccount
          error={error}
          onPaste={actions.addFromUri}
          onManual={actions.addManual}
          onDone={back}
          onTransfer={() => setScreen({ name: "transfer" })}
        />
      </main>
    );
  }

  if (screen.name === "edit") {
    // The list refreshes every second, so read the live row rather than the
    // one captured when the screen opened.
    const live =
      accounts.find((a) => a.id === screen.account.id) ?? screen.account;
    return (
      <main className="shell">
        <EditAccount
          account={live}
          folders={folders}
          error={error}
          onSave={(issuer, label) =>
            actions.update(live.id, issuer, label, null)
          }
          onMoveToFolder={(folderId) =>
            actions.moveAccountToFolder(live.id, folderId)
          }
          onDelete={() => actions.remove(live.id)}
          onDone={back}
        />
      </main>
    );
  }

  if (screen.name === "transfer") {
    return (
      <main className="shell">
        <ImportExport
          error={error}
          onImported={actions.refresh}
          onError={actions.setError}
          onDone={back}
        />
      </main>
    );
  }

  if (screen.name === "folders") {
    return (
      <main className="shell">
        <FolderEditor
          folders={folders}
          error={error}
          onCreate={actions.createFolder}
          onRename={actions.renameFolder}
          onSetIcon={actions.setFolderIcon}
          onMove={actions.moveFolder}
          onRemove={actions.removeFolder}
          onDone={back}
        />
      </main>
    );
  }

  if (screen.name === "settings" && settings) {
    return (
      <main className="shell">
        <SettingsScreen
          settings={settings}
          onSetVaultLocation={actions.setVaultLocation}
          error={error}
          onSave={actions.saveSettings}
          onLock={() => {
            void actions.lock();
            setScreen({ name: "list" });
          }}
          onDone={back}
        />
      </main>
    );
  }

  return (
    <main className="shell">
      <header className="header">
        <h1 className="header__title">Tessera</h1>
        <button
          className="header__action"
          type="button"
          onClick={() => setScreen({ name: "folders" })}
          aria-label="Folders"
          title="Folders"
        >
          <FolderIcon icon="folder" size={17} />
        </button>
        <button
          className="header__action"
          type="button"
          onClick={() => setScreen({ name: "settings" })}
          aria-label="Settings"
          title="Settings"
        >
          <FolderIcon icon="gear" size={16} />
        </button>
        <button
          className="header__action header__action--primary"
          type="button"
          onClick={() => setScreen({ name: "add" })}
          aria-label="Add an account"
          title="Add an account"
        >
          +
        </button>
      </header>

      <div className="shell__body">
        {error && <p className="error error--inline">{error}</p>}
        <AccountList
          accounts={accounts}
          folders={folders}
          clipboardClearSecs={settings?.clipboardClearSecs ?? 20}
          onEdit={(account) => setScreen({ name: "edit", account })}
          onActivity={actions.noteActivity}
        />
      </div>
    </main>
  );
}
