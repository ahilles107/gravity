import type {
  Bot,
  BotActivity,
  BotRevision,
  BusMessage,
  Conversation,
  Delivery,
  Device,
  Diagnostics,
  Project,
  Routine,
  RoutineRun,
} from "../protocol/entities";

const AT = "2024-05-01T09:00:00.000Z";

export function project(over: Partial<Project> = {}): Project {
  return { id: "p1", name: "Acme", dir_name: "acme", created_at: AT, ...over };
}

export function bot(over: Partial<Bot> = {}): Bot {
  return {
    id: "b1",
    project_id: "p1",
    name: "alice",
    description: "does things",
    avatar: "",
    instructions: "",
    state: "ready",
    state_reason: "",
    unread_count: 0,
    workspace_path: "/tmp/alice",
    dir_name: "alice",
    created_at: AT,
    ...over,
  };
}

export function botActivity(over: Partial<BotActivity> = {}): BotActivity {
  return { bot_id: "b1", from: "", text: "rules locked in", at: AT, ...over };
}

export function conversation(over: Partial<Conversation> = {}): Conversation {
  return { id: "c1", project_id: "p1", bot_id: "b1", title: "alice", ...over };
}

export function message(over: Partial<BusMessage> = {}): BusMessage {
  return {
    id: "m1",
    conversation_id: "c1",
    sender: { kind: "user", name: "you" },
    kind: "chat",
    body: "hello there",
    created_at: AT,
    ...over,
  };
}

export function delivery(over: Partial<Delivery> = {}): Delivery {
  return {
    id: "d1",
    message_id: "m1",
    bot_id: "b1",
    state: "queued",
    attempt_count: 0,
    created_at: AT,
    ...over,
  };
}

export function routine(over: Partial<Routine> = {}): Routine {
  return {
    id: "r1",
    bot_id: "b1",
    name: "weekly-report",
    trigger: { kind: "cron", expr: "0 0 9 * * MON", tz: "Europe/Warsaw" },
    prompt: "/weekly-report",
    overlap_policy: "skip",
    enabled: true,
    max_attempts: 1,
    next_run_at: AT,
    created_at: AT,
    ...over,
  };
}

export function routineRun(over: Partial<RoutineRun> = {}): RoutineRun {
  return {
    id: "rr1",
    routine_id: "r1",
    scheduled_for: AT,
    state: "succeeded",
    source: "schedule",
    attempt: 1,
    ...over,
  };
}

export function device(over: Partial<Device> = {}): Device {
  return {
    id: "dev1",
    name: "laptop",
    capabilities: ["read"],
    created_at: AT,
    ...over,
  };
}

export function diagnostics(over: Partial<Diagnostics> = {}): Diagnostics {
  return {
    daemon_version: "0.1.0",
    protocol_version: 2,
    db_healthy: true,
    runtime: { kind: "claude", available: true, version: "1.2.3" },
    delivery_backlog: 0,
    active_bots: 1,
    stale_build: false,
    uptime_seconds: 3661,
    ...over,
  };
}

export function botRevision(over: Partial<BotRevision> = {}): BotRevision {
  return {
    id: "r1",
    bot_id: "b1",
    changed_by: "bot:b1",
    field: "description",
    old_value: "does things",
    new_value: "does better things",
    created_at: AT,
    ...over,
  };
}
