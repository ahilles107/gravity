import { useCallback, useMemo, useState } from "react";
import type { ReactElement } from "react";
import { useDaemonActions } from "./app/useDaemonActions";
import { useFirstRunSetup } from "./app/useFirstRunSetup";
import type { FirstRunSetup } from "./app/useFirstRunSetup";
import { useDaemonState } from "./app/useDaemonState";
import type { DaemonState } from "./app/useDaemonState";
import { useDesktopShell } from "./app/useDesktopShell";
import { usePendingDecisions } from "./app/usePendingDecisions";
import { useOverlays } from "./app/useOverlays";
import type { OverlaysApi } from "./app/useOverlays";
import { usePaletteActions } from "./app/usePaletteActions";
import { useToasts } from "./app/useToasts";
import type { AddToast } from "./app/useToasts";
import { useUpdates } from "./app/useUpdates";
import CommandPalette from "./components/CommandPalette";
import MainPane from "./components/MainPane";
import SearchOverlay from "./components/SearchOverlay";
import SettingsOverlay from "./components/settings/SettingsOverlay";
import SetupScreen from "./components/setup/SetupScreen";
import Sidebar from "./components/Sidebar";
import Toasts from "./components/Toasts";
import type { Toast } from "./components/Toasts";
import type { DaemonApi } from "./protocol/api";
import { DaemonClient } from "./protocol/client";
import { loadEndpoint } from "./settings";
import { readClientToken } from "./token";

/** Failed deliveries per bot, for the sidebar badge. */
function countByBot(
  deliveries: readonly { readonly bot_id: string }[],
): ReadonlyMap<string, number> {
  const map = new Map<string, number>();
  for (const delivery of deliveries) {
    map.set(delivery.bot_id, (map.get(delivery.bot_id) ?? 0) + 1);
  }
  return map;
}

interface SettingsLayerProps {
  readonly client: DaemonApi;
  readonly daemon: DaemonState;
  readonly overlays: OverlaysApi;
  readonly addToast: AddToast;
}

function SettingsLayer({
  client,
  daemon,
  overlays,
  addToast,
}: SettingsLayerProps): ReactElement | null {
  if (!overlays.settingsOpen) {
    return null;
  }
  return (
    <SettingsOverlay
      client={client}
      daemon={daemon}
      category={overlays.settingsCategory}
      addToast={addToast}
      onSelectCategory={overlays.selectSettingsCategory}
      onClose={overlays.closeSettings}
    />
  );
}

interface SetupGateProps {
  readonly setup: FirstRunSetup;
  readonly toasts: readonly Toast[];
  readonly onDismissToast: (id: number) => void;
  readonly children: ReactElement;
}

function SetupGate({ setup, toasts, onDismissToast, children }: SetupGateProps): ReactElement {
  if (!setup.showWizard) {
    return children;
  }
  return (
    <div className="app">
      <SetupScreen onConnect={setup.connectToDaemon} />
      <Toasts toasts={toasts} onDismiss={onDismissToast} />
    </div>
  );
}

export default function App(): ReactElement {
  const [client] = useState(() => new DaemonClient(loadEndpoint(), readClientToken));
  const { toasts, addToast, dismissToast } = useToasts();
  const daemon = useDaemonState(client, addToast);
  const overlays = useOverlays();
  const { select, bots, canControl, connected } = daemon;

  useUpdates(addToast, daemon.status, daemon.endpoint);

  const actions = useDaemonActions({
    client,
    addToast,
    refreshAll: daemon.refreshAll,
    applyBotUpdate: daemon.applyBotUpdate,
    select,
  });

  /** Opens the bot whose DM thread backs a conversation. */
  const openConversation = useCallback(
    (conversationId: string): void => {
      const conversation = daemon.findConversation(conversationId);
      if (conversation === undefined) {
        addToast("warn", "Conversation not found", "The conversation is no longer available.");
        return;
      }
      select({ kind: "bot", botId: conversation.bot_id });
    },
    [addToast, daemon, select],
  );

  const openBot = useCallback(
    (botId: string): void => {
      select({ kind: "bot", botId });
    },
    [select],
  );

  const pending = usePendingDecisions(client, daemon.connected);

  const paletteActions = usePaletteActions({
    bots,
    select,
    openSettings: overlays.openSettings,
  });

  const failedByBot = useMemo(() => countByBot(daemon.failedDeliveries), [daemon.failedDeliveries]);

  useDesktopShell(daemon.unreadBots, pending.total, addToast);

  const setup = useFirstRunSetup(client, daemon.changeEndpoint);

  return (
    <SetupGate setup={setup} toasts={toasts} onDismissToast={dismissToast}>
      <div className="app">
        <Sidebar
          status={daemon.status}
          endpoint={daemon.endpoint}
          projects={daemon.projects}
          bots={bots}
          unreadBots={daemon.unreadBots}
          failedByBot={failedByBot}
          nextRun={daemon.nextRun}
          activityByBot={daemon.activityByBot}
          pendingDecisions={pending}
          selection={daemon.selection}
          canControl={canControl}
          onSelect={select}
          onOpenSearch={overlays.openBlankSearch}
          onCreateProject={actions.createProject}
          onCreateBot={actions.createBot}
          onDeleteBot={actions.deleteBot}
          onDeleteProject={actions.deleteProject}
          onOpenSettings={overlays.openSettings}
        />
        <main className="main">
          <MainPane
            client={client}
            daemon={daemon}
            addToast={addToast}
            onCreateProject={actions.createProjectWithBot}
            onRenameProject={actions.renameProject}
            onSetProjectLead={actions.setProjectLead}
            onDeleteProject={actions.deleteProject}
          />
        </main>
        {overlays.paletteOpen ? (
          <CommandPalette
            actions={paletteActions}
            onSearch={overlays.openSearch}
            onClose={overlays.closePalette}
          />
        ) : null}
        {overlays.searchOpen ? (
          <SearchOverlay
            client={client}
            conversations={daemon.conversations}
            bots={bots}
            projects={daemon.projects}
            connected={connected}
            initialQuery={overlays.searchQuery}
            onOpenBot={openBot}
            onOpenConversation={openConversation}
            onClose={overlays.closeSearch}
          />
        ) : null}
        <SettingsLayer client={client} daemon={daemon} overlays={overlays} addToast={addToast} />
        <Toasts toasts={toasts} onDismiss={dismissToast} />
      </div>
    </SetupGate>
  );
}
