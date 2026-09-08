import type { ReactElement } from "react";
import type { BusMessage } from "../../protocol/entities";
import { fmtTimestamp } from "../../util";

const SNIPPET_LENGTH = 160;

function snippet(body: string): string {
  const flat = body.replace(/\s+/g, " ").trim();
  return flat.length > SNIPPET_LENGTH ? `${flat.slice(0, SNIPPET_LENGTH)}…` : flat;
}

interface SearchResultRowProps {
  readonly message: BusMessage;
  readonly conversationTitle: string;
  readonly active: boolean;
  readonly onHover: () => void;
  readonly onOpen: () => void;
}

export default function SearchResultRow({
  message,
  conversationTitle,
  active,
  onHover,
  onOpen,
}: SearchResultRowProps): ReactElement {
  return (
    <button
      type="button"
      className={`overlay-option search-result ${active ? "overlay-active" : ""}`}
      onMouseEnter={onHover}
      onMouseDown={(event) => {
        event.preventDefault();
        onOpen();
      }}
    >
      <span className="search-result-meta">
        <span className="msg-sender">{message.sender.name}</span>
        <span className={`kind-badge kind-${message.kind}`}>{message.kind}</span>
        <span className="msg-time">{fmtTimestamp(message.created_at)}</span>
        <span className="search-result-conv">{conversationTitle}</span>
      </span>
      <span className="search-snippet">{snippet(message.body)}</span>
    </button>
  );
}
