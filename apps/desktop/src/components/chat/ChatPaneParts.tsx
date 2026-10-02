import { ChevronDown, ChevronUp } from "lucide-react";
import type { ReactElement } from "react";
import type { PendingMessage } from "./usePendingMessages";
import type { ChatSearch } from "./useChatSearch";

interface ChatSearchBarProps {
  readonly search: ChatSearch;
  /** Older turns exist that are not loaded, so not searched. */
  readonly hasMore: boolean;
}

function countLabel(search: ChatSearch): string {
  if (search.query.trim() === "") {
    return "";
  }
  return search.count === 0 ? "No matches" : `${search.current + 1} of ${search.count}`;
}

/** Find in the chat: the field, where you are among the matches, and the way between them. */
export function ChatSearchBar({ search, hasMore }: ChatSearchBarProps): ReactElement {
  const none = search.count === 0;
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
          } else if (event.key === "Enter") {
            event.preventDefault();
            if (event.shiftKey) {
              search.previous();
            } else {
              search.next();
            }
          }
        }}
      />
      <span className="chat-search-count" aria-live="polite">
        {countLabel(search)}
      </span>
      <button
        type="button"
        className="icon-btn"
        aria-label="Previous match"
        title="Previous match (⇧↩)"
        disabled={none}
        onClick={search.previous}
      >
        <ChevronUp size={16} aria-hidden="true" />
      </button>
      <button
        type="button"
        className="icon-btn"
        aria-label="Next match"
        title="Next match (↩)"
        disabled={none}
        onClick={search.next}
      >
        <ChevronDown size={16} aria-hidden="true" />
      </button>
      <button type="button" className="btn btn-small" onClick={() => search.toggle(false)}>
        Done
      </button>
      {hasMore && search.query.trim() !== "" ? (
        <span className="chat-search-count">Only loaded turns are searched.</span>
      ) : null}
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
