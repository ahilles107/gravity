import type { Bot, RoutineTrigger } from "../../protocol/entities";

export type TriggerKind = RoutineTrigger["kind"];

/** Draft state behind the trigger fields, before validation. */
export interface TriggerDraft {
  readonly kind: TriggerKind;
  readonly cronExpr: string;
  readonly tz: string;
  readonly intervalSeconds: string;
  readonly signalName: string;
  readonly fromBotId: string;
}

export const EMPTY_TRIGGER_DRAFT: TriggerDraft = {
  kind: "cron",
  cronExpr: "0 0 9 * * MON",
  tz: "Europe/Warsaw",
  intervalSeconds: "3600",
  signalName: "",
  fromBotId: "",
};

function cronTrigger(draft: TriggerDraft): RoutineTrigger | null {
  const expr = draft.cronExpr.trim();
  const tz = draft.tz.trim();
  return expr.length === 0 || tz.length === 0 ? null : { kind: "cron", expr, tz };
}

function intervalTrigger(draft: TriggerDraft): RoutineTrigger | null {
  const seconds = Number.parseInt(draft.intervalSeconds, 10);
  return Number.isInteger(seconds) && seconds > 0 ? { kind: "interval", seconds } : null;
}

function signalTrigger(draft: TriggerDraft): RoutineTrigger | null {
  const name = draft.signalName.trim();
  if (name.length === 0) {
    return null;
  }
  // An omitted source means any bot in the project, which is the wire default.
  return draft.fromBotId.length === 0
    ? { kind: "signal", name }
    : { kind: "signal", name, from_bot_id: draft.fromBotId };
}

/** Builds the wire trigger from a draft, or null when the draft is incomplete. */
export function buildTrigger(draft: TriggerDraft): RoutineTrigger | null {
  const builders = {
    cron: cronTrigger,
    interval: intervalTrigger,
    signal: signalTrigger,
  };
  return builders[draft.kind](draft);
}

/** Human-readable one-liner for a routine's trigger. */
export function triggerSummary(trigger: RoutineTrigger, bots: readonly Bot[]): string {
  switch (trigger.kind) {
    case "cron":
      return `cron ${trigger.expr} (${trigger.tz})`;
    case "interval":
      return `every ${trigger.seconds}s`;
    case "signal": {
      const id = trigger.from_bot_id;
      if (typeof id !== "string" || id.length === 0) {
        return `on signal ${trigger.name}`;
      }
      const from = bots.find((bot) => bot.id === id);
      return `on signal ${trigger.name} from ${from?.name ?? id}`;
    }
  }
}
