import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { agentsDaemon, browserActivity, browserTabs } from "../../test/agentFixtures";
import * as fx from "../../test/fixtures";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Bot } from "../../protocol/entities";
import { groupByTurn } from "./BrowserActivityList";
import BrowserPane from "./BrowserPane";
import { useBrowserWatch } from "./useBrowserWatch";

/** Holds the watch as the bot view does, for as long as `active`. */
function Pane({
  client,
  bot,
  active,
  connected,
}: {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly active: boolean;
  readonly connected: boolean;
}): ReactElement {
  const watch = useBrowserWatch(client, bot.id, active, connected);
  return <BrowserPane client={client} bot={bot} watch={watch} connected={connected} />;
}

const FRAME = {
  type: "browser_frame",
  bot_id: "b1",
  tab_id: "tab-1",
  data: "SlBFRw==",
  width: 800,
  height: 600,
} as const;

describe("BrowserPane", () => {
  it("watches while held, and stops when let go", async () => {
    const client = agentsDaemon();
    const view = render(<Pane client={client} bot={fx.bot()} active={false} connected />);
    await screen.findByText("Opened example.com");
    expect(client.requests.some((r) => r.body.type === "watch_browser")).toBe(false);
    view.rerender(<Pane client={client} bot={fx.bot()} active connected />);
    expect(client.requests.at(-1)?.body).toEqual({ type: "watch_browser", bot_id: "b1" });
    view.rerender(<Pane client={client} bot={fx.bot()} active={false} connected />);
    expect(client.requests.at(-1)?.body).toEqual({ type: "unwatch_browser" });
  });

  it("shows the tab on show, live, and lets the owner pick another", async () => {
    const client = agentsDaemon();
    render(<Pane client={client} bot={fx.bot()} active connected />);
    expect(await screen.findByText("Looking for the browser…")).toBeInTheDocument();
    act(() => {
      client.emit("browser_tabs", browserTabs);
      client.emit("browser_frame", FRAME);
    });
    expect(
      screen.getByText("https://example.com/pricing", { selector: ".browser-url" }),
    ).toBeInTheDocument();
    const shot = screen.getByRole("img", { name: "What alice's browser shows" });
    expect(shot).toHaveAttribute("src", "data:image/jpeg;base64,SlBFRw==");
    const tabs = screen.getByRole("tablist", { name: "Browser tabs" });
    expect(within(tabs).getByRole("tab", { name: "Pricing — Example" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    await userEvent.click(within(tabs).getByRole("tab", { name: "Docs" }));
    expect(client.requests.at(-1)?.body).toEqual({
      type: "watch_browser",
      bot_id: "b1",
      tab_id: "tab-2",
    });
    await userEvent.click(within(tabs).getByRole("button", { name: "Follow bot" }));
    expect(client.requests.at(-1)?.body).toEqual({ type: "watch_browser", bot_id: "b1" });
  });

  it("says when the browser is closed, and ignores other bots' frames", async () => {
    const client = agentsDaemon();
    render(<Pane client={client} bot={fx.bot()} active connected />);
    act(() => {
      client.emit("browser_tabs", { ...browserTabs, bot_id: "other" });
      client.emit("browser_tabs", { ...browserTabs, open: false, tabs: [], active: null });
    });
    expect(await screen.findByText(/alice's browser is closed/)).toBeInTheDocument();
  });

  it("lists each browser action under the request that led to it", async () => {
    render(<Pane client={agentsDaemon()} bot={fx.bot()} active={false} connected />);
    const log = await screen.findByRole("complementary", { name: "Browser activity" });
    expect(within(log).getByText("Task from lead")).toBeInTheDocument();
    expect(within(log).getByText("Compare the pricing tiers")).toBeInTheDocument();
    expect(within(log).getByText("your Chrome")).toBeInTheDocument();
    expect(screen.getByText(/your Chrome is off limits/)).toBeInTheDocument();
  });

  it("says when the bot may also use the owner's Chrome", async () => {
    render(
      <Pane client={agentsDaemon()} bot={fx.bot({ user_chrome: true })} active={false} connected />,
    );
    expect(await screen.findByText(/may also use your Chrome/)).toBeInTheDocument();
  });
});

describe("groupByTurn", () => {
  it("keeps a turn's actions together, newest turn first", () => {
    const groups = groupByTurn(browserActivity);
    expect(groups.map((g) => [g.turnId, g.actions.length])).toEqual([
      ["turn-2", 1],
      ["turn-1", 2],
    ]);
    expect(groups[0]?.why).toBe("You");
  });
});
