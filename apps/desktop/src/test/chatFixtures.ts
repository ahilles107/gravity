import type { ChatItem, ChatTurn, Step, TurnStats } from "../protocol/chat";

const AT = "2026-10-01T10:00:00Z";

const NO_STATS: TurnStats = {
  commands: 0,
  reads: 0,
  edits: 0,
  added: 0,
  removed: 0,
  sent: 0,
  images: 0,
  errors: 0,
};

export function stats(over: Partial<TurnStats> = {}): TurnStats {
  return { ...NO_STATS, ...over };
}

export function step(over: Partial<Step> = {}): Step {
  return {
    type: "step",
    id: "s1",
    tool: "Bash",
    title: "Run tests",
    subtitle: "cargo test",
    status: "ok",
    minor: false,
    ...over,
  };
}

export function text(markdown: string, id = "t1"): ChatItem {
  return { type: "text", id, markdown };
}

export function turn(over: Partial<ChatTurn> = {}): ChatTurn {
  return {
    id: "turn-1",
    bot_id: "b1",
    started_at: AT,
    ended_at: "2026-10-01T10:03:04Z",
    duration_ms: 184_000,
    open: false,
    trigger: { kind: "owner", text: "port the updater", via: "chat" },
    items: [text("Done — it builds.")],
    stats: stats(),
    ...over,
  };
}
