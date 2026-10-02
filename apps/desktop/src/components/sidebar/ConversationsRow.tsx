import type { ReactElement } from "react";

interface ConversationsRowProps {
  readonly selected: boolean;
  readonly onSelect: () => void;
}

function TalkIcon(): ReactElement {
  return (
    <svg viewBox="0 0 16 16" className="icon" aria-hidden="true">
      <path d="M2.5 3.5h7v5h-4l-3 2.5z" />
      <path d="M11.5 6h2v5.5l-2.5-2h-3.5" />
    </svg>
  );
}

/** Opens what the project's bots have said to each other. */
export default function ConversationsRow({
  selected,
  onSelect,
}: ConversationsRowProps): ReactElement {
  return (
    <button
      type="button"
      className={`row conversations-row ${selected ? "row-selected" : ""}`}
      title="What these bots have said to each other"
      onClick={onSelect}
    >
      <TalkIcon />
      <span className="conversations-row-name">Conversations</span>
    </button>
  );
}
