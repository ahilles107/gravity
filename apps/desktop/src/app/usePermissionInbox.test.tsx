import { act, render, renderHook, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import PermissionCards from "../components/permissions/PermissionCards";
import type { PermissionRequest } from "../protocol/chat";
import { pendingCounts } from "../test/decisionFixtures";
import { FakeDaemon } from "../test/fakeDaemon";
import * as fx from "../test/fixtures";
import type { Selection } from "./selection";
import { usePermissionInbox } from "./usePermissionInbox";
import type { AddToast } from "./useToasts";

function request(over: Partial<PermissionRequest> = {}): PermissionRequest {
  return {
    id: "r1",
    bot_id: "b1",
    tool: "Bash",
    summary: "Bash: rm -rf build",
    input: "{}",
    created_at: "2025-01-15T10:00:00Z",
    expires_at: "2025-01-15T10:10:00Z",
    ...over,
  };
}

function daemon(pending: readonly PermissionRequest[]): FakeDaemon {
  return new FakeDaemon()
    .onRequest("list_permissions", (body) => {
      expect(body).toEqual({ type: "list_permissions" });
      return { type: "permissions", req_id: "1", permissions: pending };
    })
    .onRequest("count_pending_decisions", () => ({
      type: "pending_decisions",
      req_id: "2",
      counts: pendingCounts({ total: 2, urgent: 0 }),
    }));
}

const bots = [fx.bot(), fx.bot({ id: "b2", name: "bob" })];

function inbox(client: FakeDaemon, selection: Selection) {
  const addToast = vi.fn<AddToast>();
  const select = vi.fn<(next: Selection) => void>();
  const hook = renderHook(() =>
    usePermissionInbox({ client, connected: true, bots, selection, select, addToast }),
  );
  return { hook, addToast, select };
}

describe("usePermissionInbox", () => {
  it("counts every bot's waiting prompts as urgent", async () => {
    const { hook } = inbox(daemon([request(), request({ id: "r2", bot_id: "b2" })]), {
      kind: "control",
    });
    await waitFor(() => {
      expect(hook.result.current.counts.total).toBe(4);
    });
    expect(hook.result.current.counts.urgent).toBe(2);
  });

  it("raises a toast for a prompt from a bot not on screen", async () => {
    const client = daemon([]);
    const { addToast, select } = inbox(client, { kind: "bot", botId: "b1" });
    act(() => {
      client.emit("permission_request", { type: "permission_request", request: request() });
    });
    expect(addToast).not.toHaveBeenCalled();
    act(() => {
      client.emit("permission_request", {
        type: "permission_request",
        request: request({ id: "r2", bot_id: "b2" }),
      });
    });
    expect(addToast).toHaveBeenCalledWith(
      "warn",
      "bob needs your permission",
      "Bash: rm -rf build",
      expect.objectContaining({ action: expect.objectContaining({ label: "Answer" }) }),
    );
    const options = addToast.mock.calls[0]?.[3];
    act(() => {
      options?.action?.run();
    });
    expect(select).toHaveBeenCalledWith({ kind: "control" });
  });
});

describe("PermissionCards from many bots", () => {
  it("names each card's bot and opens it", async () => {
    const onOpenBot = vi.fn<(botId: string) => void>();
    render(
      <PermissionCards
        permissions={{ pending: [request({ bot_id: "b2" })], answer: () => Promise.resolve() }}
        canAnswer
        botName={(id) => bots.find((bot) => bot.id === id)?.name}
        onOpenBot={onOpenBot}
      />,
    );
    expect(screen.getByText("bob wants to run")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Open bob" }));
    expect(onOpenBot).toHaveBeenCalledWith("b2");
  });
});
