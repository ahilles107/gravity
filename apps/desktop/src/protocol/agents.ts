// A bot's own browser (`watch_browser`, `list_browser_activity`) and the
// conversations between a project's bots (`list_agent_conversations`).

import type { StepStatus, Trigger } from "./chat";

/** A page open in a bot's browser. */
interface BrowserTab {
  readonly id: string;
  readonly title: string;
  readonly url: string;
}

/** Pushed when a watched bot's tabs change. */
export interface BrowserTabsPush {
  readonly type: "browser_tabs";
  readonly bot_id: string;
  /** False while the bot has no browser running. */
  readonly open: boolean;
  readonly tabs: readonly BrowserTab[];
  /** The tab on show. */
  readonly active: string | null;
  /** True while the view follows the tab the bot used last. */
  readonly following?: boolean;
  /** Why there is nothing to show, when it is not just a closed browser. */
  readonly reason?: string;
}

/** One screen of the tab on show, as a base64 JPEG. */
export interface BrowserFramePush {
  readonly type: "browser_frame";
  readonly bot_id: string;
  readonly tab_id: string;
  readonly data: string;
  readonly width: number;
  readonly height: number;
}

/** Whose browser a step drove. */
type BrowserKind = "own" | "owners_chrome";

/** One browser action, in the turn that made it. */
export interface BrowserAction {
  readonly turn_id: string;
  readonly step_id: string;
  readonly at: string;
  readonly browser: BrowserKind;
  readonly title: string;
  readonly subtitle?: string | null;
  readonly status: StepStatus;
  /** What started the turn: who asked, and what. */
  readonly trigger: Trigger;
}

/** A bot as a conversation names it; archived bots stay named. */
export interface AgentBot {
  readonly id: string;
  readonly name: string;
  readonly avatar: string;
  /** The peer it runs on, for a linked bot. */
  readonly machine?: string | null;
  readonly deleted: boolean;
}

/** A message from one bot to another. */
export interface AgentMessage {
  readonly id: string;
  readonly num: number;
  readonly from_bot_id: string;
  readonly to_bot_id: string;
  readonly kind: "task" | "reply" | "done" | "note" | "chat";
  readonly body: string;
  readonly ref_message_id?: string | null;
  /** Set on a task: the task it opened, and where it stands. */
  readonly task?: { readonly id: string; readonly state: string } | null;
  readonly created_at: string;
}

/** Two bots that have talked. */
export interface AgentConversation {
  readonly bot_ids: readonly [string, string];
  readonly message_count: number;
  readonly last_at: string;
  /** The newest message, its body cut to a preview. */
  readonly last: AgentMessage | null;
}
