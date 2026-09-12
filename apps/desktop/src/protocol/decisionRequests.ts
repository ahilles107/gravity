// Client → server frames for the decision registry (protocol v2).

import type { DecisionOption, DecisionPriority } from "./decisions";

/** One item in a batch publish: an optional inline ruling and who to tell. */
export interface PublishItem {
  readonly decision_id: string;
  /** Bot ids. Omit for the default set: the asker, plus whoever waits on it. */
  readonly notify_bot_ids?: readonly string[];
  /** Answer and publish in one step, for a decision with no draft. */
  readonly ruling_option?: string;
  readonly ruling_text?: string;
  readonly ruling_reason?: string;
}

export type DecisionRequestBody =
  | {
      readonly type: "list_decisions";
      readonly project_id?: string;
      /** Defaults to the working list: open plus drafted. */
      readonly state?: "open" | "settled" | "held" | "withdrawn" | "all";
      readonly tag?: string;
      readonly bot_id?: string;
      readonly query?: string;
      /** Cursor: the id of the last record on the previous page. */
      readonly before?: string;
      readonly limit?: number;
    }
  | { readonly type: "get_decision"; readonly decision_id: string }
  | { readonly type: "count_pending_decisions" }
  | {
      readonly type: "answer_decision";
      readonly decision_id: string;
      readonly ruling_option?: string;
      readonly ruling_text: string;
      readonly ruling_reason?: string;
    }
  | { readonly type: "unanswer_decision"; readonly decision_id: string }
  | {
      readonly type: "hold_decision";
      readonly decision_id: string;
      readonly until?: string;
      readonly comment?: string;
    }
  | { readonly type: "resume_decision"; readonly decision_id: string }
  | { readonly type: "confirm_decision"; readonly decision_id: string }
  | {
      readonly type: "withdraw_decision";
      readonly decision_id: string;
      readonly reason: string;
    }
  | {
      readonly type: "reopen_decision";
      readonly decision_id: string;
      readonly title?: string;
      readonly body?: string;
    }
  | {
      readonly type: "comment_decision";
      readonly decision_id: string;
      readonly body: string;
    }
  | {
      readonly type: "update_decision";
      readonly decision_id: string;
      readonly title?: string;
      readonly body?: string;
      readonly options?: readonly DecisionOption[];
      readonly recommendation?: string;
      readonly priority?: DecisionPriority;
      /** `null` clears it; omit to leave it alone. */
      readonly deadline_at?: string | null;
      readonly ruling_option?: string;
      readonly ruling_text?: string;
      readonly ruling_reason?: string;
      /** Send the edited ruling to everyone who was told the old one. */
      readonly renotify?: boolean;
    }
  | { readonly type: "delete_decision"; readonly decision_id: string }
  | { readonly type: "publish_decisions"; readonly items: readonly PublishItem[] }
  | {
      readonly type: "set_decision_tags";
      readonly decision_id: string;
      readonly tags: readonly string[];
    }
  | { readonly type: "list_tags" }
  | {
      readonly type: "upsert_tag";
      readonly name: string;
      readonly description?: string;
      readonly color?: string;
    }
  | { readonly type: "retire_tag"; readonly name: string; readonly into?: string }
  | { readonly type: "rename_tag"; readonly name: string; readonly to: string }
  | { readonly type: "delete_tag"; readonly name: string }
  | {
      readonly type: "set_project_lead";
      readonly project_id: string;
      readonly bot_id: string | null;
    };
