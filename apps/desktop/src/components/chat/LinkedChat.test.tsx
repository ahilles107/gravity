import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import * as fx from "../../test/fixtures";
import LinkedChat from "./LinkedChat";

describe("LinkedChat", () => {
  it("shows a linked bot's bus conversation and sends through it", async () => {
    const client = new FakeDaemon()
      .onRequest("list_conversations", () => ({
        type: "conversations",
        req_id: "1",
        conversations: [fx.conversation({ id: "c7", bot_id: "b1" })],
      }))
      .onRequest("list_messages", () => ({
        type: "messages",
        req_id: "2",
        messages: [
          fx.message({
            id: "m1",
            conversation_id: "c7",
            body: "build it",
            kind: "task",
            sender: { kind: "bot", name: "lead" },
          }),
        ],
      }))
      .onRequest("send_user_message", () => ({
        type: "message",
        req_id: "3",
        message: fx.message(),
      }));
    const bot = fx.bot({ peer: { id: "peer-1", name: "win-pc", online: false } });
    render(<LinkedChat client={client} bot={bot} connected writeBlocked={null} />);

    expect(await screen.findByText("build it")).toBeInTheDocument();
    expect(screen.getByText(/runs on win-pc \(offline\)/)).toBeInTheDocument();
    expect(screen.getByText("lead")).toBeInTheDocument();

    act(() => {
      client.emit("message_new", {
        type: "message_new",
        message: fx.message({
          id: "m2",
          conversation_id: "c7",
          body: "on it",
          sender: { kind: "user", name: "user" },
        }),
      });
    });
    expect(await screen.findByText("on it")).toBeInTheDocument();

    await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "status?{Enter}");
    expect(client.requests.at(-1)?.body).toEqual({
      type: "send_user_message",
      to_bot_id: "b1",
      body: "status?",
    });
  });
});
