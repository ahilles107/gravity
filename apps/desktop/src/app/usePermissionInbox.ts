import { useEffect, useMemo } from "react";
import { usePermissions } from "../components/permissions/usePermissions";
import type { Permissions } from "../components/permissions/usePermissions";
import type { DaemonApi } from "../protocol/api";
import type { PendingCounts } from "../protocol/decisions";
import type { Bot } from "../protocol/entities";
import type { Selection } from "./selection";
import { usePendingDecisions } from "./usePendingDecisions";
import type { AddToast } from "./useToasts";

interface InboxDeps {
  readonly client: DaemonApi;
  readonly connected: boolean;
  readonly bots: readonly Bot[];
  readonly selection: Selection;
  readonly select: (next: Selection) => void;
  readonly addToast: AddToast;
}

interface PermissionInbox {
  readonly permissions: Permissions;
  /** Decisions plus waiting permission prompts, which count as urgent: a bot is blocked on each. */
  readonly counts: PendingCounts;
}

/**
 * Everything waiting on the owner — decisions and every bot's permission
 * prompts — for the Control Center and the badges.
 *
 * A prompt waits on the owner wherever they are looking, so one from a bot
 * not on screen raises a toast that opens the Control Center.
 */
export function usePermissionInbox({
  client,
  connected,
  bots,
  selection,
  select,
  addToast,
}: InboxDeps): PermissionInbox {
  const decisions = usePendingDecisions(client, connected);
  const permissions = usePermissions(client, null, connected);
  const waiting = permissions.pending.length;

  const watching = selection.kind === "bot" ? selection.botId : null;
  useEffect(
    () =>
      client.on("permission_request", ({ request }) => {
        if (request.bot_id === watching) {
          return;
        }
        const name = bots.find((bot) => bot.id === request.bot_id)?.name ?? "A bot";
        addToast("warn", `${name} needs your permission`, request.summary, {
          action: { label: "Answer", run: () => select({ kind: "control" }) },
        });
      }),
    [client, bots, watching, select, addToast],
  );

  const counts = useMemo(
    () => ({
      ...decisions,
      total: decisions.total + waiting,
      urgent: decisions.urgent + waiting,
    }),
    [decisions, waiting],
  );
  return { permissions, counts };
}
