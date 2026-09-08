import type { DaemonApi } from "../protocol/api";
import type {
  Bot,
  BotActivity,
  Conversation,
  Delivery,
  Project,
  Routine,
} from "../protocol/entities";

/** Everything the client loads in one go when the connection comes up. */
export interface DaemonSnapshot {
  readonly projects: readonly Project[];
  readonly bots: readonly Bot[];
  readonly conversations: readonly Conversation[];
  readonly failedDeliveries: readonly Delivery[];
  readonly routinesByBot: readonly (readonly [string, readonly Routine[]])[];
  /** Newest turn or bus message per bot, for the sidebar preview line. */
  readonly activity: readonly BotActivity[];
}

/** Routines for one bot; an unreachable bot contributes an empty list. */
async function fetchRoutines(
  client: DaemonApi,
  botId: string,
): Promise<readonly [string, readonly Routine[]]> {
  try {
    const reply = await client.request({ type: "list_routines", bot_id: botId }, "routines");
    return [botId, reply.routines];
  } catch {
    return [botId, []];
  }
}

export async function fetchSnapshot(client: DaemonApi): Promise<DaemonSnapshot> {
  const [projects, bots, conversations, deliveries] = await Promise.all([
    client.request({ type: "list_projects" }, "projects"),
    client.request({ type: "list_bots" }, "bots"),
    client.request({ type: "list_conversations" }, "conversations"),
    client.request({ type: "list_deliveries", state: "failed" }, "deliveries"),
  ]);
  const routinesByBot = await Promise.all(bots.bots.map((bot) => fetchRoutines(client, bot.id)));
  return {
    projects: projects.projects,
    bots: bots.bots,
    conversations: conversations.conversations,
    failedDeliveries: deliveries.deliveries,
    routinesByBot,
    activity: await fetchActivity(client),
  };
}

/**
 * The sidebar preview lines, also refetched on its own whenever a bot finishes
 * a turn. A daemon that predates the request must not fail the whole snapshot,
 * so a failure here degrades to "no previews" rather than "no state".
 */
export async function fetchActivity(client: DaemonApi): Promise<readonly BotActivity[]> {
  try {
    const reply = await client.request({ type: "list_bot_activity" }, "bot_activity");
    return reply.activity;
  } catch {
    return [];
  }
}

/** Earliest `next_run_at` across a bot's enabled routines, or null when none. */
export function earliestRun(routines: readonly Routine[]): string | null {
  const upcoming = routines
    .filter((routine) => routine.enabled)
    .map((routine) => routine.next_run_at)
    .filter((at): at is string => typeof at === "string" && at.length > 0);
  return upcoming.length === 0 ? null : upcoming.reduce((a, b) => (a < b ? a : b));
}
