import type { ReactElement } from "react";
import type { PendingMessage } from "./usePendingMessages";
import type { ChatSearch } from "./useChatSearch";

interface ChatSearchBarProps {
  readonly search: ChatSearch;
  readonly shown: number;
  readonly total: number;
  readonly hasMore: boolean;
}

/** The chat's search field and how many loaded turns match. */
export function ChatSearchBar({ search, shown, total, hasMore }: ChatSearchBarProps): ReactElement {
  const further = hasMore ? " · load earlier turns to search further" : "";
  return (
    <div className="chat-search">
      <input
        className="chat-search-field"
        aria-label="Search this chat"
        placeholder="Search this chat"
        value={search.query}
        autoFocus
        onChange={(event) => {
          search.setQuery(event.target.value);
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            search.toggle(false);
          }
        }}
      />
      <span className="chat-search-count">
        {search.query.trim() === "" ? "" : `${shown} of ${total} loaded turns${further}`}
      </span>
      <button type="button" className="btn btn-small" onClick={() => search.toggle(false)}>
        Done
      </button>
    </div>
  );
}

function pendingLabel(state: PendingMessage["state"]): string {
  switch (state) {
    case "failed":
      return "Not delivered";
    case "delivered":
    case "acknowledged":
      return "Delivered — waiting for the bot to read it";
    default:
      return "Queued";
  }
}

/** Messages the owner sent that the bot has not read yet. */
export function PendingMessages({
  pending,
}: {
  readonly pending: readonly PendingMessage[];
}): ReactElement {
  return (
    <>
      {pending.map((message) => (
        <div key={message.id} className="chat-pending">
          <div className="chat-bubble chat-bubble-own">{message.text}</div>
          <div className="chat-pending-state">{pendingLabel(message.state)}</div>
        </div>
      ))}
    </>
  );
}
