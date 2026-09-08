// Client → server frames (protocol v2).

import type { DeliveryState, DeviceCapability, OverlapPolicy, RoutineTrigger } from "./entities";

export type ClientRequestBody =
  | {
      readonly type: "hello";
      readonly protocol_version: number;
      readonly token: string;
      readonly client: string;
    }
  | { readonly type: "list_projects" }
  | { readonly type: "create_project"; readonly name: string }
  | {
      readonly type: "update_project";
      readonly project_id: string;
      readonly name: string;
    }
  | { readonly type: "delete_project"; readonly project_id: string }
  | { readonly type: "list_bots"; readonly project_id?: string }
  | {
      readonly type: "create_bot";
      readonly project_id: string;
      /** Omit for an automatically numbered "New Bot" placeholder. */
      readonly name?: string;
      readonly description?: string;
      readonly instructions?: string;
      readonly avatar?: string;
    }
  | {
      readonly type: "update_bot";
      readonly bot_id: string;
      readonly name?: string;
      readonly description?: string;
      readonly instructions?: string;
      readonly avatar?: string;
    }
  | {
      readonly type: "delete_bot";
      readonly bot_id: string;
      readonly reason?: string;
    }
  | {
      readonly type: "list_bot_revisions";
      readonly bot_id: string;
      readonly limit?: number;
    }
  | { readonly type: "revert_bot_revision"; readonly revision_id: string }
  | {
      readonly type: "attach";
      readonly bot_id: string;
      readonly after_seq?: number;
    }
  | { readonly type: "detach"; readonly bot_id: string }
  | { readonly type: "input"; readonly bot_id: string; readonly data: string }
  | {
      readonly type: "resize";
      readonly bot_id: string;
      readonly cols: number;
      readonly rows: number;
      /** Repaint even when the size is unchanged; set on the resize after attach. */
      readonly force?: boolean;
    }
  | {
      readonly type: "send_user_message";
      readonly to_bot_id: string;
      readonly body: string;
    }
  | {
      readonly type: "list_messages";
      readonly conversation_id: string;
      readonly before_id?: string;
      readonly limit?: number;
    }
  | { readonly type: "list_bot_activity"; readonly project_id?: string }
  | { readonly type: "list_conversations"; readonly project_id?: string }
  | { readonly type: "list_routines"; readonly bot_id: string }
  | {
      readonly type: "create_routine";
      readonly bot_id: string;
      readonly name: string;
      readonly trigger: RoutineTrigger;
      readonly prompt: string;
      readonly overlap_policy: OverlapPolicy;
      readonly max_duration_seconds?: number;
      readonly max_attempts?: number;
    }
  | {
      readonly type: "set_routine_enabled";
      readonly routine_id: string;
      readonly enabled: boolean;
    }
  | { readonly type: "run_routine_now"; readonly routine_id: string }
  | { readonly type: "cancel_routine_run"; readonly routine_run_id: string }
  | {
      readonly type: "emit_signal";
      readonly project_id: string;
      readonly name: string;
      readonly payload?: Record<string, unknown>;
    }
  | {
      readonly type: "list_routine_runs";
      readonly routine_id: string;
      readonly limit?: number;
    }
  | {
      readonly type: "list_deliveries";
      readonly bot_id?: string;
      readonly state?: DeliveryState;
    }
  | { readonly type: "retry_delivery"; readonly delivery_id: string }
  | {
      readonly type: "search";
      readonly query: string;
      readonly project_id?: string;
    }
  | { readonly type: "diagnostics" }
  | { readonly type: "get_config" }
  | {
      readonly type: "set_config";
      /** Null clears the override so Claude Code uses the model default. */
      readonly auto_compact_window: number | null;
    }
  | { readonly type: "list_devices" }
  | {
      readonly type: "create_device";
      readonly name: string;
      readonly capabilities: readonly DeviceCapability[];
    }
  | { readonly type: "revoke_device"; readonly device_id: string };

/**
 * Requests eligible for the request/response helper: everything but the
 * handshake and the fire-and-forget frames.
 */
export type RequestBody = Exclude<ClientRequestBody, { type: "hello" | "input" | "resize" }>;

/** Fire-and-forget frames: no reply is expected. */
export type FireBody = Extract<ClientRequestBody, { type: "input" | "resize" }>;
