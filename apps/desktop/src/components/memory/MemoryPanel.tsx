import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { Bot } from "../../protocol/entities";
import { errText } from "../../util";
import ChatMarkdown from "../chat/ChatMarkdown";

/** The bot's long-term memory, in its workspace. */
export const MEMORY_FILE = "FACTS.md";

/** Waits this long after the bot's last turn update before rereading. */
const REFRESH_DELAY_MS = 800;

interface MemoryPanelProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
}

type Memory =
  | { readonly kind: "loading" }
  | { readonly kind: "loaded"; readonly text: string; readonly truncated: boolean }
  | { readonly kind: "missing" }
  | { readonly kind: "failed"; readonly error: string };

/** "file not found" means the bot has written no memory yet, not a failure. */
function isMissing(error: string): boolean {
  return error.includes("not found");
}

/**
 * What the bot keeps in `FACTS.md`: the durable facts it carries between
 * sessions. Read-only here; the bot owns the file. Rereads after the bot's
 * turns change, since that is when it writes.
 */
export default function MemoryPanel({ client, bot, connected }: MemoryPanelProps): ReactElement {
  const [memory, setMemory] = useState<Memory>({ kind: "loading" });

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request(
        { type: "read_file", bot_id: bot.id, path: MEMORY_FILE },
        "file",
      );
      setMemory({
        kind: "loaded",
        text: reply.file.text ?? "",
        truncated: reply.file.truncated,
      });
    } catch (failure) {
      const error = errText(failure);
      setMemory(isMissing(error) ? { kind: "missing" } : { kind: "failed", error });
    }
  }, [client, bot.id]);

  useLoadOnConnect(connected, load);

  useEffect(() => {
    let timer: number | undefined;
    const off = client.on("chat_turns", (push) => {
      if (push.bot_id !== bot.id) {
        return;
      }
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void load(), REFRESH_DELAY_MS);
    });
    return () => {
      window.clearTimeout(timer);
      off();
    };
  }, [client, bot.id, load]);

  return (
    <div className="memory-panel tab-pane-scroll">
      <div className="memory-head">
        <span className="memory-file">{MEMORY_FILE}</span>
        <button
          type="button"
          className="btn btn-small"
          disabled={!connected}
          onClick={() => void load()}
        >
          Refresh
        </button>
      </div>
      <MemoryBody memory={memory} name={bot.name} />
    </div>
  );
}

function MemoryBody({
  memory,
  name,
}: {
  readonly memory: Memory;
  readonly name: string;
}): ReactElement {
  switch (memory.kind) {
    case "loading":
      return <div className="muted memory-note">Loading…</div>;
    case "missing":
      return <div className="muted memory-note">{`${name} has not written any memory yet.`}</div>;
    case "failed":
      return <div className="chat-note chat-error">{memory.error}</div>;
    case "loaded":
      return (
        <div className="memory-body">
          {memory.text.trim() === "" ? (
            <div className="muted memory-note">{`${name}'s memory is empty.`}</div>
          ) : (
            <ChatMarkdown>{memory.text}</ChatMarkdown>
          )}
          {memory.truncated ? (
            <div className="muted memory-note">Only the start of the file is shown.</div>
          ) : null}
        </div>
      );
    default:
      return memory satisfies never;
  }
}
