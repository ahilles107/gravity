// The chat wire model: a bot's conversation as turns, read by the daemon from
// the runtime's transcript. See docs/superpowers/specs/2026-09-16-chat-pane-design.md.

type OwnerVia = "chat" | "terminal";

export type Trigger =
  | { readonly kind: "owner"; readonly text: string; readonly via: OwnerVia }
  | {
      readonly kind: "bus";
      readonly from: string;
      readonly msg_kind: string;
      readonly num: number;
      readonly text: string;
      readonly task_id?: string;
    }
  | {
      readonly kind: "routine";
      readonly name: string;
      readonly text: string;
      readonly run_id?: string;
    }
  | { readonly kind: "ruling"; readonly decision_id: string; readonly text: string }
  | { readonly kind: "resumed" }
  | { readonly kind: "background"; readonly text: string };

export type StepStatus = "running" | "ok" | "error";

export interface ImageRef {
  readonly id: string;
  readonly mime: string;
}

interface FileRef {
  readonly path: string;
  readonly name: string;
}

export interface Step {
  readonly type: "step";
  readonly id: string;
  readonly tool: string;
  readonly title: string;
  readonly subtitle?: string;
  readonly status: StepStatus;
  readonly minor: boolean;
  readonly added?: number;
  readonly removed?: number;
  readonly images?: readonly ImageRef[];
}

type AsideKind = "compacted" | "interrupted" | "incoming";

export type ChatItem =
  | { readonly type: "text"; readonly id: string; readonly markdown: string }
  | Step
  | {
      readonly type: "sent";
      readonly id: string;
      readonly to: string;
      readonly msg_kind: string;
      readonly body: string;
    }
  | {
      readonly type: "completed";
      readonly id: string;
      readonly task_id: string;
      readonly result: string;
      readonly artifacts: readonly FileRef[];
    }
  | {
      readonly type: "decision";
      readonly id: string;
      readonly decision_id?: string;
      readonly title: string;
    }
  | {
      readonly type: "aside";
      readonly id: string;
      readonly kind: AsideKind;
      readonly text: string;
    };

export interface TurnStats {
  readonly commands: number;
  readonly reads: number;
  readonly edits: number;
  readonly added: number;
  readonly removed: number;
  readonly sent: number;
  readonly images: number;
  readonly errors: number;
}

export interface ChatTurn {
  readonly id: string;
  readonly bot_id: string;
  readonly started_at: string;
  readonly ended_at?: string;
  readonly duration_ms?: number;
  readonly open: boolean;
  readonly trigger: Trigger;
  readonly items: readonly ChatItem[];
  readonly stats: TurnStats;
}

/** The heavy half of a step, fetched when it is opened. */
export interface StepDetail {
  readonly input?: string;
  readonly command?: string;
  readonly output?: string;
  readonly diff?: readonly string[];
  readonly content?: string;
}

export interface Artifact {
  readonly path: string;
  /** Relative to the project's artifacts directory, `/`-separated. */
  readonly rel: string;
  readonly name: string;
  readonly size: number;
  readonly modified?: string | null;
  readonly mime: string;
  readonly title?: string;
  /** Who made the file, when the daemon can tell. */
  readonly created_by?: FileCreator;
}

/** The bot (or the owner) a file came from, and how. */
export interface FileCreator {
  /** Absent for the owner. */
  readonly bot_id?: string;
  readonly name: string;
  readonly avatar: string;
  /** The machine a linked bot runs on. */
  readonly machine?: string;
  readonly via: "wrote" | "edited" | "command" | "upload" | "sent";
}

/** A file's contents: `text` when it is text, otherwise `base64`. */
export interface FileBody {
  readonly path?: string;
  readonly name: string;
  readonly mime: string;
  readonly size?: number;
  readonly text?: string;
  readonly base64?: string;
  readonly truncated: boolean;
}

/** A bot's tool waiting on the owner's answer. */
export interface PermissionRequest {
  readonly id: string;
  readonly bot_id: string;
  readonly tool: string;
  readonly summary: string;
  readonly input: string;
  readonly created_at: string;
  readonly expires_at: string;
}

export type PermissionAnswer = "allow_once" | "allow_session" | "deny";

export type PermissionOutcome =
  | "allowed_once"
  | "allowed_session"
  | "denied"
  | "expired"
  | "abandoned";
