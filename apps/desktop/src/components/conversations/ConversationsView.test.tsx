import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import {
  agentMessage,
  agentsDaemon,
  conversations,
  lead,
  projectBots,
  qa,
  thread,
  windev,
} from "../../test/agentFixtures";
import * as fx from "../../test/fixtures";
import { mergeMessages, pairKey, pairTitle, previewLine, sides } from "./conversationModel";
import ConversationsView from "./ConversationsView";

const bots = new Map([lead, windev, qa].map((bot) => [bot.id, bot]));

describe("ConversationsView", () => {
  it("lists the pairs and opens the most recent one", async () => {
    render(
      <ConversationsView
        client={agentsDaemon()}
        project={fx.project()}
        bots={projectBots}
        connected
      />,
    );
    const list = await screen.findByRole("navigation", { name: "Conversations" });
    expect(within(list).getByText("lead ↔ windev @ win-pc")).toBeInTheDocument();
    expect(within(list).getByText("lead ↔ qa")).toBeInTheDocument();
    const shown = await screen.findByRole("region", { name: "lead ↔ windev @ win-pc" });
    expect(await within(shown).findByText("Port the updater to Windows.")).toBeInTheDocument();
  });

  it("gives each bot its own bubble, face and side", async () => {
    render(
      <ConversationsView
        client={agentsDaemon()}
        project={fx.project()}
        bots={projectBots}
        connected
      />,
    );
    const first = await screen.findByText("Port the updater to Windows.");
    const fromLead = first.closest("li");
    const reply = (await screen.findByText(/Which signing certificate/)).closest("li");
    expect(fromLead).toHaveClass("conv-left");
    expect(reply).toHaveClass("conv-right");
    expect(within(fromLead ?? document.body).getByText("lead")).toBeInTheDocument();
    expect(within(reply ?? document.body).getByText("windev @ win-pc")).toBeInTheDocument();
    expect(within(fromLead ?? document.body).getByText("open")).toBeInTheDocument();
    expect(within(reply ?? document.body).getByText("reply")).toBeInTheDocument();
  });

  it("switches pairs and refreshes when bots talk", async () => {
    const client = agentsDaemon();
    render(
      <ConversationsView client={client} project={fx.project()} bots={projectBots} connected />,
    );
    await userEvent.click(await screen.findByText("lead ↔ qa"));
    const asked = client.requests.filter((r) => r.body.type === "list_agent_conversation");
    expect(asked.at(-1)?.body).toMatchObject({ bot_ids: [lead.id, qa.id] });
    const before = client.requests.length;
    act(() => {
      client.emit("message_new", {
        type: "message_new",
        message: fx.message({ sender: { kind: "bot", bot_id: lead.id, name: "lead" } }),
      });
    });
    await waitFor(
      () => {
        expect(client.requests.length).toBeGreaterThan(before);
      },
      { timeout: 2000 },
    );
  });

  it("says when no bots have talked", async () => {
    const client = agentsDaemon().onRequest("list_agent_conversations", () => ({
      type: "agent_conversations",
      req_id: "1",
      project_id: "p1",
      conversations: [],
      bots: [],
    }));
    render(<ConversationsView client={client} project={fx.project()} bots={[]} connected />);
    expect(await screen.findByText("No conversations between bots yet.")).toBeInTheDocument();
  });

  it("loads earlier messages on request", async () => {
    const client = agentsDaemon().onRequest("list_agent_conversation", (body) => ({
      type: "agent_conversation",
      req_id: "1",
      project_id: "p1",
      bot_ids: [lead.id, windev.id],
      messages:
        body.type === "list_agent_conversation" && body.before !== undefined
          ? [agentMessage({ id: "m0", num: 0, body: "Kick-off." })]
          : thread,
      has_more: !(body.type === "list_agent_conversation" && body.before !== undefined),
      bots: [lead, windev],
    }));
    render(
      <ConversationsView client={client} project={fx.project()} bots={projectBots} connected />,
    );
    await userEvent.click(await screen.findByRole("button", { name: "Load earlier messages" }));
    expect(await screen.findByText("Kick-off.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load earlier messages" })).not.toBeInTheDocument();
  });
});

describe("conversationModel", () => {
  it("keys a pair the same either way round", () => {
    expect(pairKey(["b", "a"])).toBe(pairKey(["a", "b"]));
  });

  it("merges pages once each, oldest first", () => {
    const merged = mergeMessages(thread.slice(2), thread.slice(0, 3));
    expect(merged.map((m) => m.num)).toEqual([1, 2, 3, 4]);
  });

  it("puts the bot higher in the list on the left, archived bots last", () => {
    expect(sides([windev.id, lead.id], [lead.id, windev.id])).toEqual([lead.id, windev.id]);
    expect(sides(["gone", lead.id], [lead.id])).toEqual([lead.id, "gone"]);
  });

  it("names pairs and previews the last word", () => {
    expect(pairTitle(bots, [lead.id, "gone"])).toBe("lead ↔ unknown bot");
    const [first] = conversations;
    const silent = {
      message_count: 0,
      last_at: "",
      bot_ids: [lead.id, qa.id] as const,
      last: null,
    };
    expect(first === undefined ? "" : previewLine(bots, first)).toBe(
      "windev: Ported. The updater builds and its tests pass on Windows.",
    );
    expect(previewLine(bots, silent)).toBe("");
  });
});
