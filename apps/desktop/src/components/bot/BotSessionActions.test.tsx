import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { NotifyLevel } from "../../protocol/entities";
import { agentsDaemon } from "../../test/agentFixtures";
import * as fx from "../../test/fixtures";
import BotSessionActions from "./BotSessionActions";

function daemon() {
  const client = agentsDaemon()
    .onRequest("restart_bot", () => ({ type: "ok", req_id: "1" }))
    .onRequest("clear_bot_session", () => ({ type: "ok", req_id: "1" }));
  client.capabilities = [...client.capabilities, "restart_bot"];
  return client;
}

describe("BotSessionActions", () => {
  it("restarts the bot once confirmed, saying it picks up where it left off", async () => {
    const client = daemon();
    const onToast = vi.fn<(level: NotifyLevel, title: string, body: string) => void>();
    render(
      <BotSessionActions client={client} bot={fx.bot()} connected canControl onToast={onToast} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Restart" }));
    expect(screen.getByText(/picks its conversation back up/)).toBeInTheDocument();
    await userEvent.click(screen.getAllByRole("button", { name: "Restart" }).at(-1) as HTMLElement);
    expect(client.requests.at(-1)?.body).toEqual({ type: "restart_bot", bot_id: "b1" });
    expect(onToast).toHaveBeenCalledWith("info", "Restarting", expect.stringContaining("pick up"));
  });

  it("clears the conversation once confirmed, keeping the work", async () => {
    const client = daemon();
    const onToast = vi.fn<(level: NotifyLevel, title: string, body: string) => void>();
    render(
      <BotSessionActions client={client} bot={fx.bot()} connected canControl onToast={onToast} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Clear chat" }));
    expect(screen.getByText(/its memory \(FACTS.md\) and its tasks are kept/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Clear conversation" }));
    expect(client.requests.at(-1)?.body).toEqual({ type: "clear_bot_session", bot_id: "b1" });
    expect(onToast).toHaveBeenCalledWith("info", "Conversation cleared", expect.any(String));
  });

  it("does nothing when cancelled, and reports a refusal", async () => {
    const client = daemon().onRequest("restart_bot", () => {
      throw new Error("peer is offline");
    });
    const onToast = vi.fn<(level: NotifyLevel, title: string, body: string) => void>();
    render(
      <BotSessionActions client={client} bot={fx.bot()} connected canControl onToast={onToast} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Clear chat" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(client.requests.some((r) => r.body.type === "clear_bot_session")).toBe(false);
    await userEvent.click(screen.getByRole("button", { name: "Restart" }));
    await userEvent.click(screen.getAllByRole("button", { name: "Restart" }).at(-1) as HTMLElement);
    expect(onToast).toHaveBeenCalledWith("error", "Restart failed", "peer is offline");
  });

  it("is hidden from read-only connections and older daemons", () => {
    const old = agentsDaemon();
    const { container } = render(
      <>
        <BotSessionActions
          client={daemon()}
          bot={fx.bot()}
          connected
          canControl={false}
          onToast={vi.fn<(level: NotifyLevel, title: string, body: string) => void>()}
        />
        <BotSessionActions
          client={old}
          bot={fx.bot()}
          connected
          canControl
          onToast={vi.fn<(level: NotifyLevel, title: string, body: string) => void>()}
        />
      </>,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
