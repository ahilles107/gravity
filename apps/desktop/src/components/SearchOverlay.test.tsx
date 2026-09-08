import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { FakeDaemon } from "../test/fakeDaemon";
import * as fx from "../test/fixtures";
import { voidSpy } from "../test/spies";
import SearchOverlay from "./SearchOverlay";

const RESULTS = [
  fx.message({ id: "m1", body: "  first   hit  ", conversation_id: "c1" }),
  fx.message({ id: "m2", body: "b".repeat(200), conversation_id: "c-dm" }),
  fx.message({ id: "m3", body: "third", conversation_id: "c-missing" }),
];

const BOTS = [
  fx.bot({ id: "b1", name: "alice", project_id: "p1" }),
  fx.bot({ id: "b2", name: "hitchhiker", project_id: "p2" }),
];

const PROJECTS = [fx.project({ id: "p1", name: "Acme" }), fx.project({ id: "p2", name: "Galaxy" })];

function renderOverlay(daemon: FakeDaemon, connected = true) {
  const onOpenConversation = vi.fn<(conversationId: string) => void>();
  const onOpenBot = vi.fn<(botId: string) => void>();
  const onClose = voidSpy();
  render(
    <SearchOverlay
      client={daemon}
      conversations={[
        fx.conversation({ id: "c1", title: "standup" }),
        fx.conversation({ id: "c-dm", title: "alice" }),
      ]}
      bots={BOTS}
      projects={PROJECTS}
      connected={connected}
      initialQuery=""
      onOpenBot={onOpenBot}
      onOpenConversation={onOpenConversation}
      onClose={onClose}
    />,
  );
  return { onOpenConversation, onOpenBot, onClose, input: screen.getByRole("textbox") };
}

function daemonWith(results: readonly ReturnType<typeof fx.message>[]): FakeDaemon {
  return new FakeDaemon().onRequest("search", () => ({
    type: "search_results",
    req_id: "1",
    search_results: results,
  }));
}

describe("SearchOverlay", () => {
  it("debounces the query then renders results with conversation titles", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith(RESULTS);
    const { input } = renderOverlay(daemon);

    await user.type(input, "hit");
    expect(screen.getByText("Searching…")).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText("first hit")).toBeInTheDocument();
    });
    expect(screen.getByText("standup")).toBeInTheDocument();
    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByText("unknown conversation")).toBeInTheDocument();
    expect(daemon.requests).toHaveLength(1);
  });

  it("truncates long snippets", async () => {
    const user = userEvent.setup();
    renderOverlay(daemonWith(RESULTS));
    await user.type(screen.getByRole("textbox"), "hit");
    await waitFor(() => {
      expect(screen.getByText(`${"b".repeat(160)}…`)).toBeInTheDocument();
    });
  });

  it("reports no matches", async () => {
    const user = userEvent.setup();
    renderOverlay(daemonWith([]));
    await user.type(screen.getByRole("textbox"), "nothing");
    await waitFor(() => {
      expect(screen.getByText("No messages match “nothing”.")).toBeInTheDocument();
    });
  });

  it("reports a failed search", async () => {
    const user = userEvent.setup();
    const daemon = new FakeDaemon().onRequest("search", () => {
      throw new Error("index offline");
    });
    renderOverlay(daemon);
    await user.type(screen.getByRole("textbox"), "boom");
    await waitFor(() => {
      expect(screen.getByText("Search failed: index offline")).toBeInTheDocument();
    });
  });

  it("finds bots without searching messages while disconnected", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith(RESULTS);
    renderOverlay(daemon, false);

    const input = screen.getByPlaceholderText("Search bots… (messages unavailable offline)");
    expect(input).toBeEnabled();
    await user.type(input, "hitch");

    expect(screen.getByText("hitchhiker")).toBeInTheDocument();
    expect(screen.getByText("Message search unavailable while disconnected.")).toBeInTheDocument();
    expect(daemon.requests).toHaveLength(0);
  });

  it("opens the highlighted result with the keyboard", async () => {
    const user = userEvent.setup();
    const { input, onOpenConversation, onClose } = renderOverlay(daemonWith(RESULTS));

    await user.type(input, "hit");
    await waitFor(() => {
      expect(screen.getByText("first hit")).toBeInTheDocument();
    });
    // The matching bot takes the first row, so the messages start one below.
    await user.keyboard("{ArrowDown}{ArrowDown}{Enter}");

    expect(onOpenConversation).toHaveBeenCalledWith("c-dm");
    expect(onClose).toHaveBeenCalled();
  });

  it("lists matching bots above messages with their project", async () => {
    const user = userEvent.setup();
    const { input } = renderOverlay(daemonWith(RESULTS));

    await user.type(input, "hit");
    await waitFor(() => {
      expect(screen.getByText("first hit")).toBeInTheDocument();
    });

    expect(screen.getByText("hitchhiker")).toBeInTheDocument();
    expect(screen.getByText("Galaxy")).toBeInTheDocument();
    expect(screen.queryByText("Acme")).not.toBeInTheDocument();
    const options = screen.getAllByRole("button");
    expect(options[0]).toHaveTextContent("hitchhiker");
  });

  it("activates a bot's view when its row is chosen", async () => {
    const user = userEvent.setup();
    const { input, onOpenBot, onClose } = renderOverlay(daemonWith([]));

    await user.type(input, "hitch");
    await user.click(screen.getByText("hitchhiker"));

    expect(onOpenBot).toHaveBeenCalledWith("b2");
    expect(onClose).toHaveBeenCalled();
  });

  it("shows bot matches before the message search resolves", async () => {
    const user = userEvent.setup();
    const { input } = renderOverlay(daemonWith(RESULTS));

    await user.type(input, "hitch");

    expect(screen.getByText("hitchhiker")).toBeInTheDocument();
    expect(screen.getByText("Searching…")).toBeInTheDocument();
  });

  it("opens a result clicked with the mouse", async () => {
    const user = userEvent.setup();
    const { input, onOpenConversation } = renderOverlay(daemonWith(RESULTS));

    await user.type(input, "hit");
    await waitFor(() => {
      expect(screen.getByText("first hit")).toBeInTheDocument();
    });
    await user.click(screen.getByText("first hit"));

    expect(onOpenConversation).toHaveBeenCalledWith("c1");
  });

  it("clears results when the query is emptied", async () => {
    const user = userEvent.setup();
    const { input } = renderOverlay(daemonWith(RESULTS));

    await user.type(input, "hit");
    await waitFor(() => {
      expect(screen.getByText("first hit")).toBeInTheDocument();
    });
    await user.clear(input);

    expect(screen.queryByText("first hit")).not.toBeInTheDocument();
  });

  it("closes on Escape", async () => {
    const user = userEvent.setup();
    const { onClose } = renderOverlay(daemonWith(RESULTS));
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });
});
