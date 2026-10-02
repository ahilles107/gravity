// A project's temporary workers as the Workers panel lists them (`list_workers`).

type WorkerState = "queued" | "running" | "done" | "cancelled" | "expired" | "failed";

export interface WorkerView {
  readonly id: string;
  readonly project_id: string;
  readonly name: string;
  readonly state: WorkerState;
  /** 1-based place in the project's queue, while queued. */
  readonly queue_position?: number;
  /** `here`, or the linked machine it runs on or is pinned to. */
  readonly machine?: string | null;
  readonly parent_bot_id: string;
  readonly parent_name?: string | null;
  /** The opening of the task it was given. */
  readonly brief: string;
  readonly task_id?: string;
  /** Why it waits, or why it failed or was cancelled. */
  readonly note?: string;
  readonly bot_id?: string | null;
  readonly created_at: string;
  readonly started_at?: string | null;
  readonly finished_at?: string | null;
}
