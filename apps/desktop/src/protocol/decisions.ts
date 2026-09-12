// The decision registry, as the control plane serialises it.
// See docs/protocol.md.

type DecisionKind = "question" | "decision";

/** Where a decision sits between being raised and being authority. */
type DecisionState = "open" | "answered" | "held" | "settled" | "withdrawn";

export type DecisionPriority = "normal" | "urgent";

export interface DecisionOption {
  readonly key: string;
  readonly label: string;
  /** What happens if this one is picked, in the raising bot's words. */
  readonly description?: string;
}

interface DecisionRuling {
  readonly option?: string;
  /** The owner's words. Verbatim is the norm; bots quote them. */
  readonly text: string;
  readonly reason?: string;
  readonly answered_at: string;
  /**
   * `owner`, `device:<id>`, or `owner-via-bot:<id>` when a bot filed a ruling
   * the owner gave it at its own terminal. A relay until it is confirmed.
   */
  readonly answered_by: string;
}

interface DecisionRaisedBy {
  readonly bot_id: string;
  readonly name: string;
  readonly avatar: string;
}

type CommentAuthorKind = "bot" | "user";

export interface DecisionComment {
  readonly id: string;
  readonly decision_id: string;
  readonly author_kind: CommentAuthorKind;
  readonly author_bot_id?: string;
  readonly author_name: string;
  readonly body: string;
  readonly created_at: string;
}

interface DecisionNotification {
  readonly bot_id: string;
  readonly bot_name: string;
  readonly delivery_id?: string;
  readonly created_at: string;
}

export interface Decision {
  readonly id: string;
  readonly project_id: string;
  readonly kind: DecisionKind;
  readonly title: string;
  /** Markdown. */
  readonly body: string;
  readonly options: readonly DecisionOption[];
  readonly recommendation?: string;
  readonly raised_by: DecisionRaisedBy;
  readonly on_behalf_of_bot_id?: string;
  /** Bot ids the ask travelled through, oldest first. */
  readonly origin_chain: string;
  readonly source_message_id?: string;
  readonly source_task_id?: string;
  readonly priority: DecisionPriority;
  readonly deadline_at?: string;
  readonly state: DecisionState;
  readonly held_until?: string;
  readonly ruling?: DecisionRuling;
  readonly published_at?: string;
  readonly supersedes_id?: string;
  readonly superseded_by_id?: string;
  readonly withdrawn_reason?: string;
  readonly tags: readonly string[];
  readonly comment_count: number;
  readonly last_comment_at?: string;
  /** Empty in list replies; filled for a single record. */
  readonly comments?: readonly DecisionComment[];
  readonly notifications?: readonly DecisionNotification[];
  readonly edited_at?: string;
  readonly created_at: string;
}

export interface Tag {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly color: string;
  readonly created_by: string;
  readonly retired_at?: string;
  /** Decisions filed under it, per project id. */
  readonly uses: Readonly<Record<string, number>>;
  /** Decisions carrying it that are still open, drafted or held. */
  readonly open_uses: number;
  /** Newest raise or ruling filed under it. */
  readonly last_used_at?: string;
}

/** What the sidebar badge and the dock badge count. */
export interface PendingCounts {
  readonly by_project: Readonly<Record<string, number>>;
  readonly total: number;
  readonly urgent: number;
  readonly due_soon: number;
}

/** One decision's outcome from a batch publish. */
export interface PublishResult {
  readonly decision_id: string;
  readonly notified: readonly string[];
  readonly skipped: readonly { readonly bot: string; readonly reason: string }[];
}
