// Server → client frames (protocol v2): replies carry `req_id`, pushes do not.

import type {
  Bot,
  BotActivity,
  BotRevision,
  BotState,
  BusMessage,
  Conversation,
  DaemonConfig,
  Delivery,
  Device,
  Diagnostics,
  Grant,
  NotifyLevel,
  Project,
  Routine,
  RoutineRun,
  Signal,
} from "./entities";

type ErrorCode =
  | "auth_failed"
  | "unsupported_version"
  | "not_found"
  | "invalid_request"
  | "runtime_unavailable"
  | "conflict"
  | "forbidden"
  | "internal"
  | (string & Record<never, never>);

interface ReplyBase {
  readonly req_id: string;
}

export type ServerReply =
  | (ReplyBase & {
      readonly type: "hello_ok";
      readonly protocol_version: number;
      readonly server_version: string;
      readonly capabilities: readonly string[];
      readonly grants: readonly Grant[];
      readonly device_id: string | null;
    })
  | (ReplyBase & {
      readonly type: "error";
      readonly code: ErrorCode;
      readonly message: string;
    })
  | (ReplyBase & { readonly type: "ok" })
  | (ReplyBase & { readonly type: "project"; readonly project: Project })
  | (ReplyBase & {
      readonly type: "projects";
      readonly projects: readonly Project[];
    })
  | (ReplyBase & { readonly type: "bot"; readonly bot: Bot })
  | (ReplyBase & { readonly type: "bots"; readonly bots: readonly Bot[] })
  | (ReplyBase & {
      readonly type: "bot_activity";
      readonly activity: readonly BotActivity[];
    })
  | (ReplyBase & {
      readonly type: "bot_revisions";
      readonly bot_revisions: readonly BotRevision[];
    })
  | (ReplyBase & { readonly type: "message"; readonly message: BusMessage })
  | (ReplyBase & {
      readonly type: "messages";
      readonly messages: readonly BusMessage[];
    })
  | (ReplyBase & {
      readonly type: "conversations";
      readonly conversations: readonly Conversation[];
    })
  | (ReplyBase & { readonly type: "routine"; readonly routine: Routine })
  | (ReplyBase & {
      readonly type: "routine_run";
      readonly routine_run: RoutineRun;
    })
  | (ReplyBase & { readonly type: "signal"; readonly signal: Signal })
  | (ReplyBase & {
      readonly type: "routines";
      readonly routines: readonly Routine[];
    })
  | (ReplyBase & {
      readonly type: "routine_runs";
      readonly routine_runs: readonly RoutineRun[];
    })
  | (ReplyBase & {
      readonly type: "deliveries";
      readonly deliveries: readonly Delivery[];
    })
  | (ReplyBase & {
      readonly type: "search_results";
      readonly search_results: readonly BusMessage[];
    })
  | (ReplyBase & {
      readonly type: "diagnostics";
      readonly diagnostics: Diagnostics;
    })
  | (ReplyBase & {
      readonly type: "config";
      readonly config: DaemonConfig;
    })
  | (ReplyBase & {
      readonly type: "device";
      readonly device: Device;
      /** One-time token: only present on `create_device` replies, never shown again. */
      readonly token?: string;
    })
  | (ReplyBase & {
      readonly type: "devices";
      readonly devices: readonly Device[];
    })
  | (ReplyBase & {
      readonly type: "attached";
      readonly bot_id: string;
      readonly seq: number;
      /** Whether the requested cursor was still contiguous with the server ring. */
      readonly resumed: boolean;
    });

export type ServerReplyType = ServerReply["type"];

export type ServerPush =
  | {
      readonly type: "term";
      readonly bot_id: string;
      readonly seq: number;
      readonly data: string;
    }
  | {
      readonly type: "bot_state";
      readonly bot_id: string;
      readonly state: BotState;
      readonly reason: string;
      readonly at: string;
    }
  | { readonly type: "message_new"; readonly message: BusMessage }
  | { readonly type: "bot_updated"; readonly bot: Bot }
  | { readonly type: "project_updated"; readonly project: Project }
  | { readonly type: "activity_update"; readonly activity: BotActivity }
  | { readonly type: "delivery_update"; readonly delivery: Delivery }
  | { readonly type: "routine_run_update"; readonly routine_run: RoutineRun }
  | {
      readonly type: "approval_pending";
      readonly bot_id: string;
      readonly detail: string;
    }
  | {
      readonly type: "notify";
      readonly level: NotifyLevel;
      readonly title: string;
      readonly body: string;
    };

export type ServerPushType = ServerPush["type"];

export type PushOf<K extends ServerPushType> = Extract<ServerPush, { type: K }>;
export type ReplyOf<K extends ServerReplyType> = Extract<ServerReply, { type: K }>;

export type ServerMessage = ServerReply | ServerPush;
