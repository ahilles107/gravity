// A bot's tasks as the Tasks panel lists them (`list_tasks`).

type TaskState = "open" | "done" | "cancelled" | "expired";

export interface BotTask {
  readonly id: string;
  readonly state: TaskState;
  /** `assigned`: this bot was asked; `delegated`: it asked someone else. */
  readonly role: "assigned" | "delegated";
  /** The other end: who asked, or who was asked. */
  readonly other: {
    readonly id?: string | null;
    readonly name: string;
    /** Set when the other bot runs on a peer machine. */
    readonly machine?: string | null;
  };
  readonly request: string;
  /** Set when `request` is a preview; `get_task` has all of it. */
  readonly request_truncated?: boolean;
  readonly result?: string | null;
  readonly result_truncated?: boolean;
  readonly created_at: string;
  readonly deadline_at?: string | null;
  readonly closed_at?: string | null;
}
