import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { agentsDaemon } from "../../test/agentFixtures";
import * as fx from "../../test/fixtures";
import CommandsPanel, { elapsed } from "./CommandsPanel";

describe("CommandsPanel", () => {
  it("shows what the bot is running apart from what it ran", async () => {
    render(<CommandsPanel client={agentsDaemon()} bot={fx.bot()} connected />);
    const running = await screen.findByRole("region", { name: "Running" });
    expect(within(running).getByText("Build the macOS player")).toBeInTheDocument();
    expect(within(running).getByText("background")).toBeInTheDocument();
    expect(within(running).getByText("./build.sh --player macos")).toBeInTheDocument();
    const finished = screen.getByRole("region", { name: "Finished" });
    expect(within(finished).getByText("exit 101")).toBeInTheDocument();
    expect(within(finished).getByText("done")).toBeInTheDocument();
    // A command without a description is titled by its command line.
    expect(within(finished).getAllByText("git status --short")).toHaveLength(2);
  });

  it("opens a command to its output", async () => {
    render(<CommandsPanel client={agentsDaemon()} bot={fx.bot()} connected />);
    await userEvent.click(await screen.findByRole("button", { name: /Build the macOS player/ }));
    expect(screen.getByText(/linking player/)).toBeInTheDocument();
  });

  it("copies a command and its output", async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<CommandsPanel client={agentsDaemon()} bot={fx.bot()} connected />);
    const running = await screen.findByRole("region", { name: "Running" });
    await userEvent.click(within(running).getByRole("button", { name: "Copy command" }));
    expect(writeText).toHaveBeenLastCalledWith("./build.sh --player macos");
    expect(await within(running).findByText("Copied")).toBeInTheDocument();

    await userEvent.click(within(running).getByRole("button", { name: /Build the macOS player/ }));
    await userEvent.click(within(running).getByRole("button", { name: "Copy output" }));
    expect(writeText).toHaveBeenLastCalledWith("compiling…\nlinking player");
  });

  it("refetches as the bot works", async () => {
    const client = agentsDaemon();
    render(<CommandsPanel client={client} bot={fx.bot()} connected />);
    await screen.findByText("Build the macOS player");
    const asked = (): number =>
      client.requests.filter((r) => r.body.type === "list_bot_commands").length;
    act(() => {
      client.emit("chat_turns", { type: "chat_turns", bot_id: "b1", turns: [] });
    });
    await waitFor(
      () => {
        expect(asked()).toBeGreaterThan(1);
      },
      { timeout: 2000 },
    );
  });

  it("says when there is nothing yet, and shows a failure", async () => {
    const empty = agentsDaemon().onRequest("list_bot_commands", () => ({
      type: "bot_commands",
      req_id: "1",
      bot_id: "b1",
      commands: [],
    }));
    const view = render(<CommandsPanel client={empty} bot={fx.bot()} connected />);
    expect(await screen.findByText("alice has not run any commands yet.")).toBeInTheDocument();
    view.unmount();
    const failing = agentsDaemon().onRequest("list_bot_commands", () => {
      throw new Error("peer is offline");
    });
    render(<CommandsPanel client={failing} bot={fx.bot()} connected />);
    expect(await screen.findByText("peer is offline")).toBeInTheDocument();
  });
});

describe("elapsed", () => {
  it("reads like a stopwatch", () => {
    expect(elapsed("2025-01-15T10:00:00Z", "2025-01-15T10:00:12Z")).toBe("12s");
    expect(elapsed("2025-01-15T10:00:00Z", "2025-01-15T10:04:03Z")).toBe("4m 03s");
    expect(elapsed("2025-01-15T10:00:00Z", "2025-01-15T11:05:00Z")).toBe("1h 05m");
  });
});
