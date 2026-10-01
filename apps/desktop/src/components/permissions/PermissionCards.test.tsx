import { act, render, renderHook, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { describe, expect, it } from "vitest";
import type { PermissionRequest } from "../../protocol/chat";
import { FakeDaemon } from "../../test/fakeDaemon";
import PermissionCards from "./PermissionCards";
import { usePermissions } from "./usePermissions";

function request(over: Partial<PermissionRequest> = {}): PermissionRequest {
  return {
    id: "r1",
    bot_id: "b1",
    tool: "Bash",
    summary: "Bash: rm -rf build",
    input: '{\n  "command": "rm -rf build"\n}',
    created_at: "2025-01-15T10:00:00Z",
    expires_at: "2025-01-15T10:10:00Z",
    ...over,
  };
}

function daemon(pending: readonly PermissionRequest[]): FakeDaemon {
  return new FakeDaemon()
    .onRequest("list_permissions", () => ({
      type: "permissions",
      req_id: "1",
      permissions: pending,
    }))
    .onRequest("answer_permission", () => ({
      type: "permission",
      req_id: "2",
      permission: request(),
    }));
}

function Harness({
  client,
  canAnswer = true,
}: {
  readonly client: FakeDaemon;
  readonly canAnswer?: boolean;
}): ReactElement {
  const permissions = usePermissions(client, "b1", true);
  return <PermissionCards permissions={permissions} canAnswer={canAnswer} />;
}

describe("PermissionCards", () => {
  it("answers a prompt once and removes its card", async () => {
    const client = daemon([request()]);
    render(<Harness client={client} />);
    expect(await screen.findByText("Bash: rm -rf build")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Show details/ }));
    expect(
      screen.getByText("rm -rf build", { exact: false, selector: ".tok-string" }),
    ).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Allow once/ }));
    expect(client.requests.at(-1)?.body).toEqual({
      type: "answer_permission",
      request_id: "r1",
      decision: "allow_once",
    });
    await waitFor(() => {
      expect(screen.queryByText("Bash: rm -rf build")).not.toBeInTheDocument();
    });
  });

  it("denies with a reason, and answers from the keyboard", async () => {
    const client = daemon([request(), request({ id: "r2", summary: "Write: /w/a.txt" })]);
    render(<Harness client={client} />);
    await screen.findByText("Bash: rm -rf build");

    const [firstDeny] = screen.getAllByRole("button", { name: /Deny/ });
    await userEvent.click(firstDeny ?? document.body);
    await userEvent.type(
      screen.getByRole("textbox", { name: "Reason (optional)" }),
      "keep the cache{Enter}",
    );
    expect(client.requests.at(-1)?.body).toEqual({
      type: "answer_permission",
      request_id: "r1",
      decision: "deny",
      reason: "keep the cache",
    });

    const session = await screen.findByRole("button", { name: /Allow for this session/ });
    session.focus();
    await userEvent.keyboard("s");
    expect(client.requests.at(-1)?.body).toMatchObject({
      request_id: "r2",
      decision: "allow_session",
    });
  });

  it("follows prompts pushed by the daemon", async () => {
    const client = daemon([]);
    render(<Harness client={client} />);
    act(() => {
      client.emit("permission_request", {
        type: "permission_request",
        request: request({ id: "r9" }),
      });
    });
    expect(await screen.findByText("Bash: rm -rf build")).toBeInTheDocument();
    act(() => {
      client.emit("permission_resolved", {
        type: "permission_resolved",
        request_id: "r9",
        bot_id: "b1",
        outcome: "expired",
      });
    });
    await waitFor(() => {
      expect(screen.queryByText("Bash: rm -rf build")).not.toBeInTheDocument();
    });
  });

  it("shows prompts read-only without the control grant", async () => {
    render(<Harness client={daemon([request()])} canAnswer={false} />);
    expect(await screen.findByRole("button", { name: /Allow once/ })).toBeDisabled();
    expect(screen.getByText("Read-only connection")).toBeInTheDocument();
  });

  it("treats a daemon without permissions as having none", async () => {
    const client = new FakeDaemon();
    const { result } = renderHook(() => usePermissions(client, "b1", true));
    await waitFor(() => {
      expect(result.current.pending).toEqual([]);
    });
  });
});
