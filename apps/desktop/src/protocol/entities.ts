// Domain entities exchanged over the gravityd control plane (protocol v2).
// See docs/protocol.md.

export interface Project {
  readonly id: string;
  readonly name: string;
  /** Immutable directory under `projects/`; a rename does not move it. */
  readonly dir_name: string;
  /** Set when archived: the row and its bots survive, the project does not. */
  readonly deleted_at?: string | null;
  /**
   * The bot told about every decision raised here. Not a gate — it cannot
   * answer for the owner — but without it a lead's picture of its own project
   * goes stale the moment a teammate asks directly.
   */
  readonly lead_bot_id?: string | null;
  readonly created_at: string;
}

export type BotState =
  | "starting"
  | "ready"
  | "working"
  | "waiting_for_user"
  | "waiting_for_approval"
  | "rate_limited"
  | "auth_failed"
  | "crashed"
  | "stopping"
  | "stopped";

export interface Bot {
  readonly id: string;
  readonly project_id: string;
  readonly name: string;
  readonly description: string;
  /** `icon:<name>`, `color:#rrggbb`, or empty for an id-derived swatch. */
  readonly avatar: string;
  /** Standing instructions appended to the bot's system prompt. */
  readonly instructions: string;
  readonly state: BotState;
  readonly state_reason: string;
  readonly unread_count: number;
  readonly workspace_path: string;
  /** Immutable workspace directory; a rename does not move it. */
  readonly dir_name: string;
  /** Set when another bot created this one. Provenance, not ownership. */
  readonly created_by_bot_id?: string | null;
  /** Set when archived: the row and its history survive, the bot does not. */
  readonly deleted_at?: string | null;
  readonly created_at: string;
}

/**
 * One line of "what happened last" for a bot, as the sidebar shows it.
 *
 * A bot's real conversation runs in its Claude Code terminal, which never
 * touches the message bus, so the daemon reads the newest turn out of Claude
 * Code's own transcript and reports whichever of that and the bot's newest bus
 * message is more recent.
 */
export interface BotActivity {
  readonly bot_id: string;
  /** Empty when the bot itself spoke; otherwise the sender's display name. */
  readonly from: string;
  readonly text: string;
  readonly at: string;
}

/**
 * One change to a bot's identity. Bots edit themselves without asking, so this
 * trail is what makes those changes visible and — for field edits — reversible.
 */
export interface BotRevision {
  readonly id: string;
  readonly bot_id: string;
  /** `user` or `bot:<id>`. */
  readonly changed_by: string;
  /** `created` and `deleted` are lifecycle markers rather than field edits. */
  readonly field: "name" | "avatar" | "description" | "instructions" | "created" | "deleted";
  readonly old_value: string;
  readonly new_value: string;
  readonly created_at: string;
}

/** A bot's DM thread. Every bot has exactly one, created with it. */
export interface Conversation {
  readonly id: string;
  readonly project_id: string;
  readonly bot_id: string;
  readonly title: string;
}

type SenderKind = "user" | "bot" | "routine";

interface MessageSender {
  readonly kind: SenderKind;
  readonly bot_id?: string | null;
  readonly name: string;
}

type MessageKind = "task" | "reply" | "done" | "note" | "chat";

export interface BusMessage {
  readonly id: string;
  readonly conversation_id: string;
  readonly sender: MessageSender;
  readonly kind: MessageKind;
  readonly body: string;
  readonly ref_message_id?: string | null;
  readonly created_at: string;
}

export type DeliveryState = "queued" | "leased" | "delivered" | "acknowledged" | "failed";

export interface Delivery {
  readonly id: string;
  readonly message_id: string;
  readonly bot_id: string;
  readonly state: DeliveryState;
  readonly attempt_count: number;
  readonly next_attempt_at?: string | null;
  readonly last_error?: string | null;
  readonly created_at: string;
}

export type RoutineTrigger =
  | { readonly kind: "cron"; readonly expr: string; readonly tz: string }
  | { readonly kind: "interval"; readonly seconds: number }
  | {
      readonly kind: "signal";
      readonly name: string;
      readonly from_bot_id?: string | null;
    };

export type OverlapPolicy = "skip" | "queue_one" | "queue_all" | "replace";

export interface Routine {
  readonly id: string;
  readonly bot_id: string;
  readonly name: string;
  readonly trigger: RoutineTrigger;
  readonly prompt: string;
  readonly overlap_policy: OverlapPolicy;
  readonly enabled: boolean;
  readonly max_duration_seconds?: number | null;
  readonly max_attempts: number;
  readonly next_run_at?: string | null;
  readonly created_at: string;
}

type RoutineRunState = "scheduled" | "running" | "succeeded" | "failed" | "skipped" | "cancelled";

type RunSource = "schedule" | "manual" | "signal";

export interface RoutineRun {
  readonly id: string;
  readonly routine_id: string;
  readonly scheduled_for: string;
  readonly state: RoutineRunState;
  readonly source: RunSource;
  readonly attempt: number;
  readonly signal_id?: string | null;
  readonly message_id?: string | null;
  readonly delivery_id?: string | null;
  readonly started_at?: string | null;
  readonly deadline_at?: string | null;
  readonly next_attempt_at?: string | null;
  readonly finished_at?: string | null;
  readonly error?: string | null;
}

export interface Signal {
  readonly id: string;
  readonly name: string;
  readonly source: "bot" | "manual";
  readonly project_id: string;
  readonly from_bot_id?: string | null;
  readonly payload: unknown;
  readonly origin_chain: string;
  readonly hop_count: number;
  readonly emitted_at: string;
}

interface DiagnosticsRuntime {
  readonly kind: string;
  readonly available: boolean;
  readonly version?: string | null;
}

export interface Diagnostics {
  readonly daemon_version: string;
  readonly protocol_version: number;
  /** The daemon binary on disk is newer than the running process: restart it. */
  readonly stale_build: boolean;
  readonly db_healthy: boolean;
  readonly runtime: DiagnosticsRuntime;
  readonly delivery_backlog: number;
  readonly active_bots: number;
  readonly uptime_seconds: number;
}

/**
 * Daemon configuration exposed over the control plane. Only
 * `auto_compact_window` is writable via `set_config`; the rest describe how
 * the daemon was launched and require a config-file edit plus restart.
 */
export interface DaemonConfig {
  readonly bind: readonly string[];
  /** The port being served, and the one bots reach the bus at. */
  readonly port: number;
  /** What `gravityd.toml` asked for; differs when a fallback was negotiated. */
  readonly configured_port: number;
  readonly runtime: string;
  /** Null when unset: Claude Code uses the model default. */
  readonly auto_compact_window: number | null;
}

/** Permission grants attached to a connection (from `hello_ok`) or a device. */
export type Grant = "read" | "control" | "approve";

/**
 * Capabilities a client may request when creating a device.
 *
 * `approve` is what lets a device rule on a decision; `control` alone runs the
 * fleet without being able to answer for the owner.
 */
export type DeviceCapability = Grant;

/** Every capability, in the order the device form offers them. */
export const DEVICE_CAPABILITIES: readonly DeviceCapability[] = ["read", "control", "approve"];

export interface Device {
  readonly id: string;
  readonly name: string;
  readonly capabilities: readonly Grant[];
  readonly created_at: string;
  readonly revoked_at?: string | null;
  readonly last_seen_at?: string | null;
}

export type NotifyLevel = "info" | "warn" | "error";
