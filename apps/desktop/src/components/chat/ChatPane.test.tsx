import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ChatTurn } from "../../protocol/chat";
import { step, text, turn } from "../../test/chatFixtures";
import { FakeDaemon } from "../../test/fakeDaemon";
import * as fx from "../../test/fixtures";
import ChatPane from "./ChatPane";

function daemon(turns: readonly ChatTurn[], hasMore = false): FakeDaemon {
  return new FakeDaemon()
    .onRequest("list_chat", (body) => ({
      type: "chat",
      req_id: "1",
      bot_id: "b1",
      turns: body.type === "list_chat" && body.before !== undefined ? [olderTurn] : turns,
      has_more: body.type === "list_chat" && body.before !== undefined ? false : hasMore,
    }))
    .onRequest("send_user_message", (body) => ({
      type: "message",
      req_id: "2",
      message: fx.message({ id: "m9", body: body.type === "send_user_message" ? body.body : "" }),
    }));
}

const olderTurn = turn({
  id: "turn-0",
  started_at: "2026-10-01T09:00:00Z",
  trigger: { kind: "bus", from: "lead", msg_kind: "task", num: 2, text: "an older task" },
  items: [],
});

function renderPane(client: FakeDaemon, over: Partial<Parameters<typeof ChatPane>[0]> = {}) {
  const onOpenFile = vi.fn<(path: string) => void>();
  render(
    <ChatPane
      client={client}
      bot={fx.bot()}
      connected
      writeBlocked={null}
      onOpenFile={onOpenFile}
      {...over}
    />,
  );
  return { onOpenFile };
}

describe("ChatPane", () => {
  it("shows turns with their trigger, text and stats", async () => {
    renderPane(
      daemon([
        turn({
          trigger: {
            kind: "bus",
            from: "lead",
            msg_kind: "task",
            num: 4,
            text: "port the updater",
          },
          stats: {
            commands: 2,
            reads: 0,
            edits: 1,
            added: 3,
            removed: 1,
            sent: 0,
            images: 0,
            errors: 0,
          },
        }),
      ]),
    );
    expect(await screen.findByText("Task from lead")).toBeInTheDocument();
    expect(screen.getByText("port the updater")).toBeInTheDocument();
    expect(screen.getByText("Done — it builds.")).toBeInTheDocument();
    expect(screen.getByText("2 commands · 1 edit +3 −1")).toBeInTheDocument();
  });

  it("follows the bot live and pages back on request", async () => {
    const client = daemon([turn({ open: true, items: [] })], true);
    renderPane(client);
    expect(await screen.findByText("Working…")).toBeInTheDocument();

    act(() => {
      client.emit("chat_turns", {
        type: "chat_turns",
        bot_id: "b1",
        turns: [turn({ open: false, items: [text("All done.")] })],
      });
    });
    expect(await screen.findByText("All done.")).toBeInTheDocument();
    expect(screen.queryByText("Working…")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Load earlier turns" }));
    expect(await screen.findByText("an older task")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load earlier turns" })).not.toBeInTheDocument();
  });

  it("sends from the composer and shows the message until the bot reads it", async () => {
    const client = daemon([]);
    renderPane(client);
    const field = await screen.findByRole("textbox", { name: "Message" });
    await userEvent.type(field, "which SDK?{Enter}");

    expect(client.requests.at(-1)?.body).toEqual({
      type: "send_user_message",
      to_bot_id: "b1",
      body: "which SDK?",
    });
    expect(await screen.findByText("which SDK?")).toBeInTheDocument();
    expect(screen.getByText("Queued")).toBeInTheDocument();
    expect(field).toHaveValue("");

    act(() => {
      client.emit("delivery_update", {
        type: "delivery_update",
        delivery: fx.delivery({ message_id: "m9", state: "delivered" }),
      });
    });
    expect(await screen.findByText(/waiting for the bot to read it/)).toBeInTheDocument();

    act(() => {
      client.emit("chat_turns", {
        type: "chat_turns",
        bot_id: "b1",
        turns: [turn({ trigger: { kind: "owner", text: "which SDK?", via: "chat" } })],
      });
    });
    await waitFor(() => {
      expect(screen.queryByText(/waiting for the bot/)).not.toBeInTheDocument();
    });
    expect(screen.getByText("You")).toBeInTheDocument();
  });

  it("explains why the composer is disabled", async () => {
    renderPane(daemon([]), { writeBlocked: "Read-only connection" });
    const field = await screen.findByRole("textbox", { name: "Message" });
    expect(field).toBeDisabled();
    expect(field).toHaveAttribute("placeholder", "Read-only connection");
    expect(screen.getByText(/Nothing here yet/)).toBeInTheDocument();
  });

  it("opens a step's detail and a result's files", async () => {
    const client = daemon([
      turn({
        items: [
          step({
            id: "edit-1",
            tool: "Edit",
            title: "Edited app.ts",
            subtitle: "/w/app.ts",
            added: 1,
            removed: 1,
          }),
          {
            type: "completed",
            id: "c1",
            task_id: "t1",
            result: "Ported.",
            artifacts: [{ path: "/p/artifacts/report.md", name: "report.md" }],
          },
        ],
      }),
    ]).onRequest("get_chat_step", () => ({
      type: "chat_step",
      req_id: "3",
      bot_id: "b1",
      item_id: "edit-1",
      detail: { diff: ["@@ -1,1 +1,1 @@", "-old", "+new"] },
    }));
    const { onOpenFile } = renderPane(client);

    await userEvent.click(await screen.findByRole("button", { name: /Edited app.ts/ }));
    const diff = await screen.findByLabelText("Changes");
    expect(within(diff).getByText("+new")).toHaveClass("diff-add");
    expect(within(diff).getByText("-old")).toHaveClass("diff-del");

    await userEvent.click(screen.getByRole("button", { name: "report.md" }));
    expect(onOpenFile).toHaveBeenCalledWith("/p/artifacts/report.md");
  });

  it("shows step images fetched from the daemon", async () => {
    const client = daemon([
      turn({ items: [step({ id: "shot", images: [{ id: "img-1", mime: "image/png" }] })] }),
    ]).onRequest("get_chat_image", () => ({
      type: "file",
      req_id: "4",
      file: { name: "img-1", mime: "image/png", base64: "iVBORw0KGgo=", truncated: false },
    }));
    renderPane(client);
    const open = await screen.findByRole("button", { name: "Open image" });
    expect(within(open).getByRole("presentation")).toHaveAttribute(
      "src",
      "data:image/png;base64,iVBORw0KGgo=",
    );
    await userEvent.click(open);
    expect(screen.getByRole("dialog", { name: "Image" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Image" })).not.toBeInTheDocument();
  });

  it("reports a daemon that cannot serve the chat", async () => {
    renderPane(new FakeDaemon());
    expect(await screen.findByText(/no fake responder/)).toBeInTheDocument();
  });
});

describe("ChatPane commands and search", () => {
  it("types slash commands into the terminal instead of the bus", async () => {
    const client = daemon([]);
    renderPane(client);
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Message" }),
      "/compact{Enter}",
    );
    expect(client.fired[0]).toEqual({
      type: "input",
      bot_id: "b1",
      data: "\u001b[200~/compact\u001b[201~",
    });
    expect(client.requests.some((r) => r.body.type === "send_user_message")).toBe(false);
  });

  it("narrows the turns to a search with ⌘F", async () => {
    renderPane(
      daemon([
        turn({ id: "a", items: [text("Built the installer.", "x")] }),
        turn({
          id: "b",
          started_at: "2026-10-01T11:00:00Z",
          items: [text("Signed the MSI.", "y")],
        }),
      ]),
    );
    expect(await screen.findByText("Built the installer.")).toBeInTheDocument();
    await userEvent.keyboard("{Meta>}f{/Meta}");
    await userEvent.type(screen.getByRole("textbox", { name: "Search this chat" }), "msi");
    expect(screen.queryByText("Built the installer.")).not.toBeInTheDocument();
    expect(screen.getByText("Signed the MSI.")).toBeInTheDocument();
    expect(screen.getByText("1 of 2 loaded turns")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.getByText("Built the installer.")).toBeInTheDocument();
  });
});
