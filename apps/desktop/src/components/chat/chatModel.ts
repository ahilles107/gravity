// Pure helpers for the chat pane: merging live turns, grouping steps, and the
// words a turn is introduced with.

import type { ChatItem, ChatTurn, Step, Trigger, TurnStats } from "../../protocol/chat";

/**
 * Folds pushed or paged turns into the list, replacing turns by id and keeping
 * the list in start order. A pushed turn is usually the open one changing.
 */
export function mergeTurns(
  current: readonly ChatTurn[],
  incoming: readonly ChatTurn[],
): readonly ChatTurn[] {
  if (incoming.length === 0) {
    return current;
  }
  const byId = new Map(current.map((turn) => [turn.id, turn]));
  for (const turn of incoming) {
    byId.set(turn.id, turn);
  }
  const merged = [...byId.values()];
  // `sort` on a fresh copy, not `toSorted`: the build targets ES2021.
  // oxlint-disable-next-line unicorn/no-array-sort
  merged.sort((a, b) => (a.started_at === b.started_at ? 0 : a.started_at < b.started_at ? -1 : 1));
  return merged;
}

/** A turn's items as the pane lays them out: runs of steps fold together. */
export type ChatBlock =
  | { readonly kind: "steps"; readonly id: string; readonly steps: readonly Step[] }
  | { readonly kind: "item"; readonly item: Exclude<ChatItem, Step> };

export function groupItems(items: readonly ChatItem[]): readonly ChatBlock[] {
  const blocks: ChatBlock[] = [];
  let run: Step[] = [];
  const flush = (): void => {
    const first = run[0];
    if (first !== undefined) {
      blocks.push({ kind: "steps", id: first.id, steps: run });
    }
    run = [];
  };
  for (const item of items) {
    if (item.type === "step") {
      run.push(item);
    } else {
      flush();
      blocks.push({ kind: "item", item });
    }
  }
  flush();
  return blocks;
}

/** A step group starts open while short, or while its turn is still running. */
export function groupStartsOpen(steps: readonly Step[], turnOpen: boolean): boolean {
  return turnOpen || steps.filter((step) => !step.minor).length <= 4;
}

export interface TriggerView {
  /** The turn's headline, e.g. "Task from lead". */
  readonly label: string;
  /** What the trigger said, shown as a bubble; empty when there is nothing to show. */
  readonly text: string;
  /** True when the owner spoke: the bubble sits on the right. */
  readonly own: boolean;
}

const BUS_KIND_LABEL: Readonly<Record<string, string>> = {
  task: "Task from",
  reply: "Reply from",
  note: "Note from",
  done: "Result from",
};

export function triggerView(trigger: Trigger): TriggerView {
  switch (trigger.kind) {
    case "owner":
      return {
        label: trigger.via === "chat" ? "You" : "You, in the terminal",
        text: trigger.text,
        own: true,
      };
    case "bus":
      return {
        label: `${BUS_KIND_LABEL[trigger.msg_kind] ?? "Message from"} ${trigger.from}`,
        text: trigger.text,
        own: false,
      };
    case "routine":
      return { label: `Routine ${trigger.name}`, text: trigger.text, own: false };
    case "ruling":
      return { label: "Your ruling", text: trigger.text, own: false };
    case "resumed":
      return { label: "Continued", text: "", own: false };
    case "background":
      return { label: "Background", text: trigger.text, own: false };
    default:
      return trigger satisfies never;
  }
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** "3 commands · 2 edits +120 −4 · 1 message sent", or empty for a quiet turn. */
export function statsLine(stats: TurnStats): string {
  const parts: string[] = [];
  if (stats.commands > 0) {
    parts.push(plural(stats.commands, "command", "commands"));
  }
  if (stats.edits > 0) {
    const lines = stats.added + stats.removed > 0 ? ` +${stats.added} −${stats.removed}` : "";
    parts.push(`${plural(stats.edits, "edit", "edits")}${lines}`);
  }
  if (stats.reads > 0) {
    parts.push(plural(stats.reads, "read", "reads"));
  }
  if (stats.sent > 0) {
    parts.push(`${plural(stats.sent, "message", "messages")} sent`);
  }
  if (stats.images > 0) {
    parts.push(plural(stats.images, "image", "images"));
  }
  if (stats.errors > 0) {
    parts.push(plural(stats.errors, "error", "errors"));
  }
  return parts.join(" · ");
}

/** "6 steps · 3 edits +120 −4" for a folded step group. */
export function groupSummary(steps: readonly Step[]): string {
  const edits = steps.filter((step) => step.added !== undefined || step.removed !== undefined);
  const added = edits.reduce((sum, step) => sum + (step.added ?? 0), 0);
  const removed = edits.reduce((sum, step) => sum + (step.removed ?? 0), 0);
  const failed = steps.filter((step) => step.status === "error").length;
  const parts = [plural(steps.length, "step", "steps")];
  if (edits.length > 0) {
    parts.push(`${plural(edits.length, "edit", "edits")} +${added} −${removed}`);
  }
  if (failed > 0) {
    parts.push(plural(failed, "error", "errors"));
  }
  return parts.join(" · ");
}

/** "took 3m 4s", "took 12s", or empty when unknown. */
export function durationLabel(ms: number | undefined): string {
  if (ms === undefined) {
    return "";
  }
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) {
    return `took ${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return rest === 0 ? `took ${minutes}m` : `took ${minutes}m ${rest}s`;
}
