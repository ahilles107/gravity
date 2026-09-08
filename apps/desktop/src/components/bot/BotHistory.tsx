import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Bot, BotRevision, NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";

/**
 * A bot's identity history.
 *
 * Bots change their own name, avatar, description and instructions without
 * asking anyone, and create and delete each other. Nothing prompts the user, so
 * this list is how those changes stay visible — and, for field edits, undoable.
 */

interface BotHistoryProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly canControl: boolean;
  readonly onBotUpdated: (bot: Bot) => void;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

/** Lifecycle markers are history, not state: there is nothing to restore. */
const LIFECYCLE = new Set<BotRevision["field"]>(["created", "deleted"]);

function author(changedBy: string): string {
  return changedBy === "user" ? "you" : "a bot";
}

function summarise(revision: BotRevision): string {
  const who = author(revision.changed_by);
  if (revision.field === "created") {
    return `Created by ${who}`;
  }
  if (revision.field === "deleted") {
    return `Deleted by ${who}`;
  }
  return `${revision.field} changed by ${who}`;
}

function preview(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length === 0) {
    return "(empty)";
  }
  return trimmed.length > 80 ? `${trimmed.slice(0, 80)}…` : trimmed;
}

export default function BotHistory({
  client,
  bot,
  canControl,
  onBotUpdated,
  onToast,
}: BotHistoryProps): ReactElement {
  const [revisions, setRevisions] = useState<readonly BotRevision[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [reverting, setReverting] = useState<string | null>(null);

  const load = async (): Promise<void> => {
    setLoading(true);
    try {
      const reply = await client.request(
        { type: "list_bot_revisions", bot_id: bot.id },
        "bot_revisions",
      );
      setRevisions(reply.bot_revisions);
    } catch (error) {
      onToast("error", "Could not load history", errText(error));
    } finally {
      setLoading(false);
    }
  };

  const revert = async (revision: BotRevision): Promise<void> => {
    setReverting(revision.id);
    try {
      const reply = await client.request(
        { type: "revert_bot_revision", revision_id: revision.id },
        "bot",
      );
      onBotUpdated(reply.bot);
      onToast("info", "Reverted", `${revision.field} restored.`);
      await load();
    } catch (error) {
      onToast("error", "Revert failed", errText(error));
    } finally {
      setReverting(null);
    }
  };

  return (
    <details
      className="bot-history"
      onToggle={(event) => {
        if (event.currentTarget.open && revisions === null && !loading) {
          void load();
        }
      }}
    >
      <summary>History</summary>
      {loading ? <p className="muted">Loading…</p> : null}
      {revisions !== null && revisions.length === 0 ? (
        <p className="muted">No changes recorded yet.</p>
      ) : null}
      <ul className="revision-list">
        {(revisions ?? []).map((revision) => (
          <li key={revision.id} className="revision">
            <div className="revision-head">
              <span className="revision-summary">{summarise(revision)}</span>
              <time className="revision-at">{revision.created_at}</time>
            </div>
            {LIFECYCLE.has(revision.field) ? null : (
              <div className="revision-diff">
                <span className="revision-old">{preview(revision.old_value)}</span>
                <span aria-hidden="true"> → </span>
                <span className="revision-new">{preview(revision.new_value)}</span>
              </div>
            )}
            {LIFECYCLE.has(revision.field) ? null : (
              <button
                type="button"
                className="btn btn-small"
                disabled={!canControl || reverting !== null}
                onClick={() => {
                  void revert(revision);
                }}
              >
                {reverting === revision.id ? "Reverting…" : "Revert"}
              </button>
            )}
          </li>
        ))}
      </ul>
    </details>
  );
}
