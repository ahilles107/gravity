import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { agentsDaemon, browserTabs } from "../test/agentFixtures";
import * as fx from "../test/fixtures";
import { actionToastSpy, botSpy, routinesSpy, stubLocalStorage } from "../test/spies";
import BotView from "./BotView";

vi.mock("./TerminalPane", () => ({
  default: (): React.ReactElement => <div data-testid="terminal" />,
}));

function renderView(): ReturnType<typeof agentsDaemon> {
  const client = agentsDaemon()
    .onRequest("list_routines", () => ({ type: "routines", req_id: "1", routines: [] }))
    .onRequest("list_chat", () => ({
      type: "chat",
      req_id: "2",
      bot_id: "b1",
      has_more: false,
      turns: [],
    }));
  render(
    <BotView
      client={client}
      bot={fx.bot()}
      bots={[fx.bot()]}
      connected
      canControl
      onBotUpdated={botSpy()}
      onRoutinesChanged={routinesSpy()}
      onToast={actionToastSpy()}
    />,
  );
  return client;
}

describe("BotView with its own browser", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("streams the bot's browser from the moment it is selected", async () => {
    const client = renderView();
    await waitFor(() => {
      expect(client.requests.some((r) => r.body.type === "watch_browser")).toBe(true);
    });
    // Still on the chat: the stream is already live, and the tab says so.
    const browserTab = screen.getByRole("button", { name: /^Browser/ });
    expect(screen.queryByRole("img", { name: "live" })).not.toBeInTheDocument();
    act(() => {
      client.emit("browser_tabs", browserTabs);
      client.emit("browser_frame", {
        type: "browser_frame",
        bot_id: "b1",
        tab_id: "tab-1",
        data: "SlBFRw==",
        width: 800,
        height: 600,
      });
    });
    expect(screen.getByRole("img", { name: "live" })).toBeInTheDocument();
    await userEvent.click(browserTab);
    expect(screen.getByRole("img", { name: "What alice's browser shows" })).toBeInTheDocument();
  });

  it("shows the bot's memory beside its info", async () => {
    renderView();
    await userEvent.click(screen.getByRole("button", { name: "Memory" }));
    expect(await screen.findByRole("heading", { name: "Facts" })).toBeInTheDocument();
  });
});
