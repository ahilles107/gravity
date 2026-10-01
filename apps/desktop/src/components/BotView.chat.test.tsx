import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { turn } from "../test/chatFixtures";
import { FakeDaemon } from "../test/fakeDaemon";
import * as fx from "../test/fixtures";
import { actionToastSpy, botSpy, routinesSpy, stubLocalStorage } from "../test/spies";
import BotView from "./BotView";

vi.mock("./TerminalPane", () => ({
  default: (): React.ReactElement => <div data-testid="terminal" />,
}));

function chatDaemon(): FakeDaemon {
  const client = new FakeDaemon()
    .onRequest("list_routines", () => ({ type: "routines", req_id: "1", routines: [] }))
    .onRequest("list_chat", () => ({
      type: "chat",
      req_id: "2",
      bot_id: "b1",
      has_more: false,
      turns: [
        turn({
          items: [
            {
              type: "completed",
              id: "c1",
              task_id: "t1",
              result: "Ported.",
              artifacts: [{ path: "/p/artifacts/report.md", name: "report.md" }],
            },
          ],
        }),
      ],
    }))
    .onRequest("read_file", () => ({
      type: "file",
      req_id: "3",
      file: { name: "report.md", mime: "text/markdown", text: "Report body", truncated: false },
    }))
    .onRequest("list_conversations", () => ({
      type: "conversations",
      req_id: "4",
      conversations: [],
    }));
  client.capabilities = ["terminal_attach", "chat"];
  return client;
}

function renderView(client: FakeDaemon, bot = fx.bot()): void {
  render(
    <BotView
      client={client}
      bot={bot}
      bots={[bot]}
      connected
      canControl
      onBotUpdated={botSpy()}
      onRoutinesChanged={routinesSpy()}
      onToast={actionToastSpy()}
    />,
  );
}

describe("BotView with chat", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("opens on the chat and keeps the terminal a tab away", async () => {
    renderView(chatDaemon());
    expect(screen.getByRole("button", { name: "Chat" })).toHaveClass("tab-active");
    expect(await screen.findByText("Ported.")).toBeInTheDocument();
    expect(screen.getByTestId("terminal")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "2", metaKey: true });
    expect(screen.getByRole("button", { name: "Terminal" })).toHaveClass("tab-active");

    fireEvent.keyDown(window, { key: "l", metaKey: true });
    expect(screen.getByRole("button", { name: "Chat" })).toHaveClass("tab-active");
    await waitFor(() => {
      expect(screen.getByRole("textbox", { name: "Message" })).toHaveFocus();
    });
  });

  it("opens a result's file in the Files panel", async () => {
    renderView(chatDaemon());
    await userEvent.click(await screen.findByRole("button", { name: "report.md" }));
    expect(screen.getByRole("button", { name: "Files" })).toHaveClass("tab-active");
    expect(await screen.findByText("Report body")).toBeInTheDocument();
  });

  it("gives a linked bot only its chat", () => {
    renderView(chatDaemon(), fx.bot({ peer: { id: "x", name: "win-pc", online: true } }));
    expect(screen.getByRole("button", { name: "Chat" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Terminal" })).not.toBeInTheDocument();
    expect(screen.queryByTestId("terminal")).not.toBeInTheDocument();
  });

  it("falls back to the terminal on a daemon without chat", () => {
    const client = chatDaemon();
    client.capabilities = ["terminal_attach"];
    renderView(client);
    expect(screen.queryByRole("button", { name: "Chat" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Terminal" })).toHaveClass("tab-active");
  });
});
