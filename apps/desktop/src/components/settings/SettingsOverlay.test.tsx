import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DaemonState } from "../../app/useDaemonState";
import { DEFAULT_PREFS, getPrefs, reloadPrefsForTest } from "../../prefs";
import { FakeDaemon } from "../../test/fakeDaemon";
import { stubLocalStorage, voidSpy } from "../../test/spies";
import type { SettingsCategory } from "./categories";
import SettingsOverlay from "./SettingsOverlay";

function daemonState(client: FakeDaemon): DaemonState {
  return {
    status: "connected",
    endpoint: client.getEndpoint(),
    serverVersion: client.serverVersion,
    canControl: true,
    connected: true,
    projects: [],
    bots: [],
    conversations: [],
    failedDeliveries: [],
    unreadBots: {},
    nextRun: {},
    activityByBot: {},
    selection: { kind: "none" },
    select: voidSpy(),
    changeEndpoint: vi.fn<(endpoint: { host: string; port: number }) => void>(),
    refreshAll: vi.fn<() => Promise<void>>(() => Promise.resolve()),
    updateBotRoutines: vi.fn<DaemonState["updateBotRoutines"]>(),
    applyBotUpdate: vi.fn<DaemonState["applyBotUpdate"]>(),
    applyProjectUpdate: vi.fn<DaemonState["applyProjectUpdate"]>(),
    findConversation: () => undefined,
  };
}

function renderOverlay(category: SettingsCategory = "connection") {
  const client = new FakeDaemon().onRequest("get_config", () => ({
    type: "config" as const,
    req_id: "1",
    config: {
      bind: ["127.0.0.1"],
      port: 49777,
      configured_port: 49777,
      runtime: "pty",
      auto_compact_window: 250_000,
    },
  }));
  const onSelectCategory = vi.fn<(category: SettingsCategory) => void>();
  const onClose = voidSpy();
  render(
    <SettingsOverlay
      client={client}
      daemon={daemonState(client)}
      category={category}
      addToast={vi.fn<(level: "info" | "warn" | "error", title: string, body: string) => void>()}
      onSelectCategory={onSelectCategory}
      onClose={onClose}
    />,
  );
  return { client, onSelectCategory, onClose };
}

describe("SettingsOverlay", () => {
  beforeEach(() => {
    stubLocalStorage();
    reloadPrefsForTest();
  });

  it("shows the category rail and the selected pane", () => {
    renderOverlay();
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Connection" })).toBeInTheDocument();
    expect(screen.getByLabelText("Daemon host")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Connection" })).toHaveFocus();
  });

  it("shows the daemon config inside the connection pane", async () => {
    renderOverlay();
    expect(await screen.findByLabelText("Auto-compact window")).toHaveValue("250000");
    expect(screen.getByText("49777")).toBeInTheDocument();
  });

  it("switches category from the rail", async () => {
    const user = userEvent.setup();
    const { onSelectCategory } = renderOverlay();

    await user.click(screen.getByRole("button", { name: "General" }));

    expect(onSelectCategory).toHaveBeenCalledWith("general");
  });

  it("renders the general pane with the font, notification and shortcut controls", () => {
    renderOverlay("general");
    expect(screen.getByLabelText("Terminal font size")).toBeInTheDocument();
    expect(screen.getByLabelText("Terminal font")).toBeInTheDocument();
    expect(screen.getByLabelText("Dock badge")).toBeChecked();
    expect(screen.getByRole("button", { name: "Show or hide window shortcut" })).toHaveTextContent(
      "Not set",
    );
  });

  it("commits the terminal font only after editing finishes", async () => {
    const user = userEvent.setup();
    renderOverlay("general");
    const input = screen.getByLabelText("Terminal font");

    await user.clear(input);
    await user.type(input, "Menlo");
    expect(getPrefs().terminalFontFamily).toBe(DEFAULT_PREFS.terminalFontFamily);

    await user.tab();
    expect(getPrefs().terminalFontFamily).toBe("Menlo");
  });

  it("closes on Escape and via the close button", async () => {
    const user = userEvent.setup();
    const { onClose } = renderOverlay();

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: "Close settings" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
