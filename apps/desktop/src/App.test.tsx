import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { FakeDaemon } from "./test/fakeDaemon";
import * as fx from "./test/fixtures";
import { stubLocalStorage } from "./test/spies";

let daemon: FakeDaemon;
let storage: Map<string, string>;

const UNREAD_KEY = "gravity.unread";
const LAST_USED_BOT_KEY = "gravity.last-used-bot";
const AT = "2024-05-01T09:00:00.000Z";

vi.mock("./components/TerminalPane", () => ({
  default: (): React.ReactElement => <div data-testid="terminal" />,
}));
// `new DaemonClient(...)` yields the prepared double: a constructor that
// returns an object produces that object.
vi.mock("./protocol/client", () => ({
  DaemonClient: function DaemonClientDouble(): FakeDaemon {
    return daemon;
  },
}));
vi.mock("./token", () => ({ readClientToken: () => Promise.resolve("token") }));

function seedDaemon(): FakeDaemon {
  return new FakeDaemon()
    .onRequest("list_projects", () => ({ type: "projects", req_id: "1", projects: [fx.project()] }))
    .onRequest("list_bots", () => ({
      type: "bots",
      req_id: "1",
      bots: [fx.bot()],
    }))
    .onRequest("list_conversations", () => ({
      type: "conversations",
      req_id: "1",
      conversations: [fx.conversation()],
    }))
    .onRequest("list_deliveries", () => ({ type: "deliveries", req_id: "1", deliveries: [] }))
    .onRequest("list_bot_activity", () => ({ type: "bot_activity", req_id: "1", activity: [] }))
    .onRequest("list_routines", () => ({ type: "routines", req_id: "1", routines: [fx.routine()] }))
    .onRequest("list_messages", () => ({ type: "messages", req_id: "1", messages: [] }));
}

/**
 * Adds a second bot, bob, with its own DM thread.
 *
 * Startup opens the first row, so anything about an *unselected* bot needs a
 * bot that is not alice.
 */
function seedSecondBot(): void {
  daemon
    .onRequest("list_bots", () => ({
      type: "bots",
      req_id: "1",
      bots: [fx.bot(), fx.bot({ id: "b2", name: "bob" })],
    }))
    .onRequest("list_conversations", () => ({
      type: "conversations",
      req_id: "1",
      conversations: [fx.conversation(), fx.conversation({ id: "c2", bot_id: "b2", title: "bob" })],
    }));
}

/** The sidebar row for a bot, distinct from the same name in the open view. */
function botRow(name: string): HTMLElement {
  return screen.getByText(name, { selector: ".bot-row-name" });
}

/** Renders the app and waits for the initial snapshot to land. */
async function renderApp(): Promise<void> {
  render(<App />);
  daemon.setStatus("connected");
  await waitFor(() => {
    expect(screen.getByText("Acme")).toBeInTheDocument();
  });
  // The snapshot paints one commit before the startup pick runs, so flush that
  // effect too - otherwise assertions race an empty main pane.
  await act(async () => {});
}

/** Seeds the badges a previous session left behind. */
function storeUnread(entries: Readonly<Record<string, { count: number; seenAt: number }>>): void {
  storage.set(UNREAD_KEY, JSON.stringify(entries));
}

function readUnread(): Record<string, { count: number; seenAt: number }> {
  return JSON.parse(storage.get(UNREAD_KEY) ?? "{}") as Record<
    string,
    { count: number; seenAt: number }
  >;
}

describe("App state", () => {
  beforeEach(() => {
    daemon = seedDaemon();
    daemon.status = "disconnected";
    storage = stubLocalStorage();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("loads the daemon snapshot on connect", async () => {
    seedSecondBot();
    await renderApp();
    expect(daemon.started).toBe(true);
    expect(botRow("alice")).toBeInTheDocument();
  });

  it("restores the badges the previous session left unread", async () => {
    seedSecondBot();
    storeUnread({ b2: { count: 2, seenAt: Date.parse(AT) } });
    await renderApp();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("keeps a bot read across a reload when nothing new was said", async () => {
    // The daemon's own `unread_count` tracks what a bot has yet to consume, so
    // seeding badges from it brought every read message back on every reload.
    seedSecondBot();
    daemon
      .onRequest("list_bots", () => ({
        type: "bots",
        req_id: "1",
        bots: [fx.bot(), fx.bot({ id: "b2", name: "bob", unread_count: 7 })],
      }))
      .onRequest("list_bot_activity", () => ({
        type: "bot_activity",
        req_id: "1",
        activity: [fx.botActivity({ bot_id: "b2", text: "said before", at: AT })],
      }));
    storeUnread({ b2: { count: 0, seenAt: Date.parse(AT) } });
    await renderApp();
    expect(screen.getByText("said before")).toBeInTheDocument();
    expect(screen.queryByText("7")).not.toBeInTheDocument();
    expect(screen.queryByText("1")).not.toBeInTheDocument();
  });

  it("badges a bot that spoke while the client was closed", async () => {
    seedSecondBot();
    daemon.onRequest("list_bot_activity", () => ({
      type: "bot_activity",
      req_id: "1",
      activity: [fx.botActivity({ bot_id: "b2", at: "2024-05-01T10:00:00.000Z" })],
    }));
    storeUnread({ b2: { count: 0, seenAt: Date.parse(AT) } });
    await renderApp();
    expect(screen.getByText("1")).toBeInTheDocument();
  });

  it("opens the first sidebar bot on a first launch", async () => {
    await renderApp();
    // A fresh launch would otherwise land on a blank main pane.
    expect(screen.getByText("alice", { selector: ".view-title" })).toBeInTheDocument();
    expect(storage.get(LAST_USED_BOT_KEY)).toBe("b1");
  });

  it("opens the first sidebar bot when the last-used bot no longer exists", async () => {
    storage.set(LAST_USED_BOT_KEY, "deleted-bot");
    await renderApp();
    expect(screen.getByText("alice", { selector: ".view-title" })).toBeInTheDocument();
    expect(screen.queryByText("2")).not.toBeInTheDocument();
    expect(storage.get(LAST_USED_BOT_KEY)).toBe("b1");
  });

  it("opens the last-used bot on startup", async () => {
    seedSecondBot();
    storage.set(LAST_USED_BOT_KEY, "b2");

    await renderApp();

    expect(screen.getByText("bob", { selector: ".view-title" })).toBeInTheDocument();
  });

  it("updates the sidebar preview when a finished turn is pushed", async () => {
    // A terminal turn never touches the bus, so this push is the only signal
    // the sidebar gets. The daemon holds it until the transcript is readable.
    await renderApp();
    expect(screen.getByText("does things", { selector: ".bot-row-preview" })).toBeInTheDocument();

    daemon.emit("activity_update", {
      type: "activity_update",
      activity: fx.botActivity({ text: "PROBE-OK" }),
    });

    await waitFor(() => {
      expect(screen.getByText("PROBE-OK")).toBeInTheDocument();
    });
  });

  it("shows the empty state when there is no bot to open", async () => {
    daemon.onRequest("list_bots", () => ({ type: "bots", req_id: "1", bots: [] }));
    await renderApp();
    expect(screen.getByText(/Select a bot/)).toBeInTheDocument();
    // The endpoint lives in the sidebar footer alone; the pane stays quiet.
    expect(screen.queryByText(/Connected to/)).not.toBeInTheDocument();
  });

  it("reports a snapshot failure as a toast", async () => {
    daemon.onRequest("list_bots", () => {
      throw new Error("daemon busy");
    });
    render(<App />);
    daemon.setStatus("connected");
    await waitFor(() => {
      expect(screen.getByText("Failed to load daemon state")).toBeInTheDocument();
    });
    expect(screen.getByText("daemon busy")).toBeInTheDocument();
  });

  it("opens a bot and clears its unread badge for good", async () => {
    const user = userEvent.setup();
    seedSecondBot();
    storeUnread({ b2: { count: 2, seenAt: 0 } });
    await renderApp();

    await user.click(botRow("bob"));
    expect(screen.getByTestId("terminal")).toBeInTheDocument();
    // A control connection owns the terminal outright: no read-only badge.
    expect(screen.queryByText("Read-only")).not.toBeInTheDocument();
    expect(screen.queryByText("2")).not.toBeInTheDocument();
    // Persisted, so the next launch does not resurrect what was just read.
    expect(readUnread()["b2"]?.count).toBe(0);
    expect(storage.get(LAST_USED_BOT_KEY)).toBe("b2");
  });

  it("raises an unread badge for a message in an unselected bot's DM", async () => {
    seedSecondBot();
    storeUnread({ b2: { count: 2, seenAt: 0 } });
    await renderApp();
    daemon.emit("message_new", {
      type: "message_new",
      message: fx.message({
        conversation_id: "c2",
        sender: { kind: "bot", bot_id: "b1", name: "alice" },
      }),
    });
    await waitFor(() => {
      expect(screen.getByText("3")).toBeInTheDocument();
    });
  });

  it("raises an unread badge when an unselected bot finishes a turn", async () => {
    // A bot answering in its terminal never reaches the bus, so the preview
    // push is the only signal that it said anything at all.
    seedSecondBot();
    await renderApp();
    daemon.emit("activity_update", {
      type: "activity_update",
      activity: fx.botActivity({ bot_id: "b2", text: "on it", at: "2024-05-01T10:00:00.000Z" }),
    });
    await waitFor(() => {
      expect(screen.getByText("1")).toBeInTheDocument();
    });
  });

  it("raises no badge for the bot the user is looking at", async () => {
    await renderApp();
    // Startup opens alice's row one snapshot after the project lands, and this
    // is about activity for the bot already on screen: emitting before that
    // selection commits would count it against a bot nobody was looking at.
    await waitFor(() => {
      expect(screen.getByTestId("terminal")).toBeInTheDocument();
    });
    daemon.emit("activity_update", {
      type: "activity_update",
      activity: fx.botActivity({ text: "on it", at: "2024-05-01T10:00:00.000Z" }),
    });
    await waitFor(() => {
      expect(screen.getByText("on it")).toBeInTheDocument();
    });
    expect(screen.queryByText("1")).not.toBeInTheDocument();
  });

  it("ignores a message for an unknown conversation", async () => {
    seedSecondBot();
    storeUnread({ b2: { count: 2, seenAt: 0 } });
    await renderApp();
    daemon.emit("message_new", {
      type: "message_new",
      message: fx.message({ conversation_id: "nope" }),
    });
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("applies bot state pushes", async () => {
    const user = userEvent.setup();
    await renderApp();
    await user.click(botRow("alice"));

    daemon.emit("bot_state", {
      type: "bot_state",
      bot_id: "b1",
      state: "working",
      reason: "thinking",
      at: "now",
    });
    // The header shows routine states through the dot alone, so the label is
    // the only place the push surfaces.
    await waitFor(() => {
      expect(screen.getByLabelText("working \u2014 thinking")).toBeInTheDocument();
    });
  });

  it("adds a bot created by another bot to the sidebar", async () => {
    await renderApp();
    expect(screen.queryByText("steve")).not.toBeInTheDocument();

    // Creation, rename and deletion all arrive as `bot_updated`; only the
    // snapshot ever lists bots, so an unhandled push leaves the sidebar stale
    // until the next reconnect.
    daemon.emit("bot_updated", {
      type: "bot_updated",
      bot: fx.bot({ id: "b2", name: "steve" }),
    });
    await waitFor(() => {
      expect(screen.getByText("steve")).toBeInTheDocument();
    });
  });

  it("updates the sidebar and inspector on bot_updated, then removes archived bots", async () => {
    await renderApp();

    daemon.emit("bot_updated", {
      type: "bot_updated",
      bot: fx.bot({ name: "alice-renamed", avatar: "icon:rune" }),
    });
    await waitFor(() => {
      expect(botRow("alice-renamed")).toBeInTheDocument();
      expect(screen.getByDisplayValue("alice-renamed")).toBeInTheDocument();
      expect(screen.getByRole("radio", { name: "rune" })).toBeChecked();
    });
    expect(screen.queryByText("alice", { selector: ".bot-row-name" })).not.toBeInTheDocument();

    // An archived bot arrives as the same push, carrying `deleted_at`.
    daemon.emit("bot_updated", {
      type: "bot_updated",
      bot: fx.bot({ name: "alice-renamed", deleted_at: "2024-05-01T09:00:00.000Z" }),
    });
    await waitFor(() => {
      expect(
        screen.queryByText("alice-renamed", { selector: ".bot-row-name" }),
      ).not.toBeInTheDocument();
    });
  });

  it("shows notify pushes as toasts and dismisses them", async () => {
    const user = userEvent.setup();
    await renderApp();

    daemon.emit("notify", { type: "notify", level: "warn", title: "Disk low", body: "5% left" });
    await waitFor(() => {
      expect(screen.getByText("Disk low")).toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText("Disk low")).not.toBeInTheDocument();
  });

  it("offers a View action on approval pushes that opens the bot", async () => {
    const user = userEvent.setup();
    await renderApp();

    daemon.emit("approval_pending", {
      type: "approval_pending",
      bot_id: "b1",
      detail: "wants to run rm",
    });
    await waitFor(() => {
      expect(screen.getByText("alice is waiting for approval")).toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: "View" }));
    expect(screen.queryByText("wants to run rm")).not.toBeInTheDocument();
    expect(screen.getByTestId("terminal")).toBeInTheDocument();
  });

  it("tracks failed deliveries pushed by the daemon", async () => {
    await renderApp();
    daemon.emit("delivery_update", {
      type: "delivery_update",
      delivery: fx.delivery({ state: "failed" }),
    });
    await waitFor(() => {
      expect(screen.getByTitle("1 failed deliveries")).toBeInTheDocument();
    });

    daemon.emit("delivery_update", {
      type: "delivery_update",
      delivery: fx.delivery({ state: "acknowledged" }),
    });
    await waitFor(() => {
      expect(screen.queryByTitle("1 failed deliveries")).not.toBeInTheDocument();
    });
  });
});
