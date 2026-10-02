import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { agentsDaemon } from "../../test/agentFixtures";
import * as fx from "../../test/fixtures";
import MemoryPanel, { MEMORY_FILE } from "./MemoryPanel";

describe("MemoryPanel", () => {
  it("shows the bot's FACTS.md as markdown", async () => {
    const client = agentsDaemon();
    render(<MemoryPanel client={client} bot={fx.bot()} connected />);
    expect(await screen.findByText("release.pfx", { exact: false })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Facts" })).toBeInTheDocument();
    expect(client.requests[0]?.body).toEqual({
      type: "read_file",
      bot_id: "b1",
      path: MEMORY_FILE,
    });
  });

  it("says so when the bot has written nothing yet", async () => {
    const client = agentsDaemon().onRequest("read_file", () => {
      throw new Error("file not found: FACTS.md");
    });
    render(<MemoryPanel client={client} bot={fx.bot()} connected />);
    expect(await screen.findByText("alice has not written any memory yet.")).toBeInTheDocument();
  });

  it("shows a real failure, and an empty file as empty", async () => {
    const failing = agentsDaemon().onRequest("read_file", () => {
      throw new Error("permission denied");
    });
    const view = render(<MemoryPanel client={failing} bot={fx.bot()} connected />);
    expect(await screen.findByText("permission denied")).toBeInTheDocument();
    view.unmount();
    const empty = agentsDaemon().onRequest("read_file", () => ({
      type: "file",
      req_id: "1",
      file: { name: "FACTS.md", mime: "text/markdown", text: " ", truncated: true },
    }));
    render(<MemoryPanel client={empty} bot={fx.bot()} connected />);
    expect(await screen.findByText("alice's memory is empty.")).toBeInTheDocument();
    expect(screen.getByText("Only the start of the file is shown.")).toBeInTheDocument();
  });

  it("rereads after the bot's turns change, and on Refresh", async () => {
    const client = agentsDaemon();
    render(<MemoryPanel client={client} bot={fx.bot()} connected />);
    await screen.findByRole("heading", { name: "Facts" });
    const reads = (): number => client.requests.filter((r) => r.body.type === "read_file").length;
    act(() => {
      client.emit("chat_turns", { type: "chat_turns", bot_id: "other", turns: [] });
      client.emit("chat_turns", { type: "chat_turns", bot_id: "b1", turns: [] });
    });
    await waitFor(
      () => {
        expect(reads()).toBe(2);
      },
      { timeout: 2000 },
    );
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(reads()).toBe(3);
  });
});
