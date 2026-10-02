import { useCallback, useEffect, useRef, useState } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { PermissionAnswer, PermissionRequest } from "../../protocol/chat";

export interface Permissions {
  readonly pending: readonly PermissionRequest[];
  readonly answer: (id: string, decision: PermissionAnswer, reason?: string) => Promise<void>;
}

/**
 * A bot's permission prompts waiting on the owner: listed on connect, then
 * followed through `permission_request` and `permission_resolved` pushes.
 * A daemon without the capability answers with an error; that is no prompts.
 */
export function usePermissions(client: DaemonApi, botId: string, connected: boolean): Permissions {
  const [pending, setPending] = useState<readonly PermissionRequest[]>([]);
  // Prompts pushed while a list request is in flight: the reply predates them.
  const pushed = useRef(new Set<string>());

  const load = useCallback(async (): Promise<void> => {
    pushed.current = new Set();
    let listed: readonly PermissionRequest[] = [];
    try {
      const reply = await client.request(
        { type: "list_permissions", bot_id: botId },
        "permissions",
      );
      listed = reply.permissions;
    } catch {
      // A daemon without permissions has none to show.
    }
    const ids = new Set(listed.map((r) => r.id));
    setPending((current) => [
      ...listed,
      ...current.filter((r) => pushed.current.has(r.id) && !ids.has(r.id)),
    ]);
  }, [client, botId]);

  useLoadOnConnect(connected, load);

  useEffect(() => {
    const off = [
      client.on("permission_request", (push) => {
        if (push.request.bot_id === botId) {
          pushed.current.add(push.request.id);
          setPending((current) => [
            ...current.filter((r) => r.id !== push.request.id),
            push.request,
          ]);
        }
      }),
      client.on("permission_resolved", (push) => {
        pushed.current.delete(push.request_id);
        setPending((current) => current.filter((r) => r.id !== push.request_id));
      }),
    ];
    return () => {
      for (const unsubscribe of off) {
        unsubscribe();
      }
    };
  }, [client, botId]);

  const answer = useCallback(
    async (id: string, decision: PermissionAnswer, reason?: string): Promise<void> => {
      await client.request(
        {
          type: "answer_permission",
          request_id: id,
          decision,
          ...(reason === undefined ? {} : { reason }),
        },
        "permission",
      );
      setPending((current) => current.filter((r) => r.id !== id));
    },
    [client],
  );

  return { pending, answer };
}
