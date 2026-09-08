import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FakeDaemon } from "../test/fakeDaemon";
import * as fx from "../test/fixtures";
import { actionToastSpy, botSpy, routinesSpy, stubLocalStorage } from "../test/spies";
import BotView from "./BotView";

vi.mock("./TerminalPane", () => ({
  default: (): React.ReactElement => <div data-testid="terminal" />,
}));

function renderView(over: Partial<Parameters<typeof BotView>[0]> = {}) {
  const daemon = new FakeDaemon().onRequest("list_routines", () => ({
    type: "routines",
    req_id: "1",
    routines: [],
  }));
  const props = {
    client: daemon,
    bot: fx.bot(),
    bots: [fx.bot()],
    connected: true,
    canControl: true,
    onBotUpdated: botSpy(),
    onRoutinesChanged: routinesSpy(),
    onToast: actionToastSpy(),
    ...over,
  };
  render(<BotView {...props} />);
  return props;
}

describe("BotView", () => {
  let storage = new Map<string, string>();

  beforeEach(() => {
    vi.clearAllMocks();
    storage = stubLocalStorage();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows the bot state without a read-only badge for control connections", () => {
    renderView({ bot: fx.bot({ state: "waiting_for_approval" }) });
    expect(screen.getByText("waiting for approval")).toBeInTheDocument();
    expect(screen.queryByText("Read-only")).not.toBeInTheDocument();
  });

  it("offers no lifecycle controls: bots are always-on", () => {
    renderView();
    expect(screen.queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Start" })).not.toBeInTheDocument();

    renderView({ bot: fx.bot({ state: "crashed" }) });
    expect(screen.queryByRole("button", { name: "Start" })).not.toBeInTheDocument();
  });

  it("makes the header a window drag strip", () => {
    renderView();
    const header = screen.getByText("alice").closest(".view-header");
    expect(header).toHaveAttribute("data-tauri-drag-region", "deep");
  });

  it("shows the read-only badge without the control grant", () => {
    renderView({ canControl: false });
    expect(screen.getByText("Read-only")).toBeInTheDocument();
  });

  it("keeps bot info beside both tabs while preserving the terminal", async () => {
    const user = userEvent.setup();
    renderView();
    expect(screen.getByTestId("terminal")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Bot info" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Info" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Routines" }));
    expect(screen.getByText("Routines", { selector: ".panel-title" })).toBeInTheDocument();
    expect(screen.getByTestId("terminal")).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Bot info" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Terminal" }));
    expect(screen.getByText("Bot info", { selector: ".panel-title" })).toBeInTheDocument();
  });

  it("collapses and restores the inspector without losing draft edits", async () => {
    const user = userEvent.setup();
    renderView();
    const panel = screen.getByRole("complementary", { name: "Bot info" });
    const name = screen.getByDisplayValue("alice");

    await user.clear(name);
    await user.type(name, "Ada");
    await user.click(screen.getByRole("button", { name: "Collapse bot info" }));

    expect(panel).toHaveClass("bot-info-panel-collapsed");
    expect(screen.getByRole("button", { name: "Expand bot info" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );

    await user.click(screen.getByRole("button", { name: "Expand bot info" }));
    expect(panel).not.toHaveClass("bot-info-panel-collapsed");
    expect(name).toHaveValue("Ada");
    await waitFor(() => {
      expect(storage.get("gravity.bot-info-panel")).toBe(
        JSON.stringify({ collapsed: false, width: 360 }),
      );
    });
  });

  it("resizes the inspector with the keyboard and resets on double-click", async () => {
    const user = userEvent.setup();
    renderView();
    const panel = screen.getByRole("complementary", { name: "Bot info" });
    const separator = screen.getByRole("separator", { name: "Resize bot info" });

    separator.focus();
    await user.keyboard("{ArrowLeft}");
    expect(panel).toHaveStyle({ width: "370px" });

    fireEvent.pointerDown(separator, { pointerId: 1, clientX: 500 });
    fireEvent.pointerMove(separator, { pointerId: 1, clientX: 450 });
    fireEvent.pointerUp(separator, { pointerId: 1 });
    expect(panel).toHaveStyle({ width: "420px" });

    await user.dblClick(separator);
    expect(panel).toHaveStyle({ width: "360px" });
  });

  it("restores the saved inspector layout", () => {
    storage.set("gravity.bot-info-panel", JSON.stringify({ collapsed: true, width: 420 }));
    renderView();

    expect(screen.getByRole("complementary", { name: "Bot info" })).toHaveClass(
      "bot-info-panel-collapsed",
    );
    expect(screen.getByRole("complementary", { name: "Bot info" })).toHaveStyle({
      width: "420px",
    });
  });
});
