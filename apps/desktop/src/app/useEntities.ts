import { useCallback, useState } from "react";
import type {
  Bot,
  BotState,
  BotActivity,
  Conversation,
  Delivery,
  Project,
  Routine,
} from "../protocol/entities";
import { withoutKey } from "./records";
import { earliestRun } from "./snapshot";
import type { DaemonSnapshot } from "./snapshot";

type Stamps = Readonly<Record<string, string>>;
type Activities = Readonly<Record<string, BotActivity>>;

export interface EntitiesApi {
  readonly projects: readonly Project[];
  readonly bots: readonly Bot[];
  readonly conversations: readonly Conversation[];
  readonly failedDeliveries: readonly Delivery[];
  readonly nextRun: Stamps;
  /** Newest turn or bus message per bot, keyed by bot id. */
  readonly activityByBot: Activities;
  readonly applySnapshot: (snapshot: DaemonSnapshot) => void;
  readonly applyProjectUpdate: (project: Project) => void;
  readonly applyBotState: (botId: string, state: BotState, reason: string) => void;
  readonly applyBotUpdate: (bot: Bot) => void;
  readonly applyDelivery: (delivery: Delivery) => void;
  readonly setBotRoutines: (botId: string, routines: readonly Routine[]) => void;
  readonly applyActivity: (activity: readonly BotActivity[]) => void;
  /** Replaces one bot's preview line, from an `activity_update` push. */
  readonly applyBotActivity: (activity: BotActivity) => void;
}

function byBot(activity: readonly BotActivity[]): Activities {
  return Object.fromEntries(activity.map((item) => [item.bot_id, item]));
}

/** The entity lists mirrored from the daemon, plus their push-driven updates. */
export function useEntities(): EntitiesApi {
  const [projects, setProjects] = useState<readonly Project[]>([]);
  const [bots, setBots] = useState<readonly Bot[]>([]);
  const [conversations, setConversations] = useState<readonly Conversation[]>([]);
  const [failedDeliveries, setFailedDeliveries] = useState<readonly Delivery[]>([]);
  const [nextRun, setNextRun] = useState<Stamps>({});
  const [activityByBot, setActivityByBot] = useState<Activities>({});

  const setBotRoutines = useCallback((botId: string, routines: readonly Routine[]): void => {
    const earliest = earliestRun(routines);
    setNextRun((prev) =>
      earliest === null ? withoutKey(prev, botId) : { ...prev, [botId]: earliest },
    );
  }, []);

  const applySnapshot = useCallback((snapshot: DaemonSnapshot): void => {
    setProjects(snapshot.projects);
    setBots(snapshot.bots);
    setConversations(snapshot.conversations);
    setFailedDeliveries(snapshot.failedDeliveries);
    const runs: Record<string, string> = {};
    for (const [botId, routines] of snapshot.routinesByBot) {
      const earliest = earliestRun(routines);
      if (earliest !== null) {
        runs[botId] = earliest;
      }
    }
    setNextRun(runs);
    setActivityByBot(byBot(snapshot.activity));
  }, []);

  /**
   * Upsert a project from a `project_updated` push or a request reply.
   *
   * Creation, rename and deletion all arrive as this one push, so the daemon's
   * `ORDER BY name` is rebuilt here the same way `applyBotUpdate` does it.
   */
  const applyProjectUpdate = useCallback((project: Project): void => {
    setProjects((prev) => {
      const others = prev.filter((item) => item.id !== project.id);
      if (typeof project.deleted_at === "string") {
        return others;
      }
      const at = others.findIndex((item) => item.name.localeCompare(project.name) > 0);
      return at === -1
        ? [...others, project]
        : [...others.slice(0, at), project, ...others.slice(at)];
    });
  }, []);

  const applyBotState = useCallback((botId: string, state: BotState, reason: string): void => {
    setBots((prev) =>
      prev.map((bot) => (bot.id === botId ? { ...bot, state, state_reason: reason } : bot)),
    );
  }, []);

  /**
   * Upsert a bot from a `bot_updated` push or a request reply.
   *
   * Creation, rename and deletion all arrive as this one push, so an update
   * for a bot the list has never seen is the normal case for a bot built by
   * another bot — replacing in place alone would drop it silently. The daemon
   * lists live bots ordered by name, so the order is rebuilt to match and a
   * rename lands in the right place.
   */
  const applyBotUpdate = useCallback((bot: Bot): void => {
    setBots((prev) => {
      const others = prev.filter((item) => item.id !== bot.id);
      if (typeof bot.deleted_at === "string") {
        return others;
      }
      // The remaining bots are still in the daemon's `ORDER BY name`, so
      // splicing this one in before the first name that sorts after it keeps
      // the list matching a fresh `list_bots` — including after a rename.
      const at = others.findIndex((item) => item.name.localeCompare(bot.name) > 0);
      return at === -1 ? [...others, bot] : [...others.slice(0, at), bot, ...others.slice(at)];
    });
  }, []);

  const applyActivity = useCallback((activity: readonly BotActivity[]): void => {
    setActivityByBot(byBot(activity));
  }, []);

  const applyBotActivity = useCallback((activity: BotActivity): void => {
    setActivityByBot((prev) => ({ ...prev, [activity.bot_id]: activity }));
  }, []);

  const applyDelivery = useCallback((delivery: Delivery): void => {
    setFailedDeliveries((prev) => {
      const others = prev.filter((item) => item.id !== delivery.id);
      return delivery.state === "failed" ? [...others, delivery] : others;
    });
  }, []);

  return {
    projects,
    bots,
    conversations,
    failedDeliveries,
    nextRun,
    activityByBot,
    applySnapshot,
    applyProjectUpdate,
    applyBotState,
    applyBotUpdate,
    applyDelivery,
    setBotRoutines,
    applyActivity,
    applyBotActivity,
  };
}
