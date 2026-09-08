import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactElement } from "react";
import { useListKeyboardNav } from "../hooks/useListKeyboardNav";
import { useScrollActiveIntoView } from "../hooks/useScrollActiveIntoView";
import type { DaemonApi } from "../protocol/api";
import type { Bot, BusMessage, Conversation, Project } from "../protocol/entities";
import OverlayShell from "./overlay/OverlayShell";
import BotResultRow from "./search/BotResultRow";
import SearchResultRow from "./search/SearchResultRow";
import { rankBots } from "./search/botMatches";
import type { BotMatch } from "./search/botMatches";
import { useMessageSearch } from "./search/useMessageSearch";

interface SearchOverlayProps {
  readonly client: DaemonApi;
  readonly conversations: readonly Conversation[];
  readonly bots: readonly Bot[];
  readonly projects: readonly Project[];
  readonly connected: boolean;
  readonly initialQuery: string;
  readonly onOpenBot: (botId: string) => void;
  readonly onOpenConversation: (conversationId: string) => void;
  readonly onClose: () => void;
}

/** One navigable row: bots rank above messages, so Enter reaches both. */
type SearchItem =
  | { readonly kind: "bot"; readonly match: BotMatch }
  | { readonly kind: "message"; readonly message: BusMessage };

function titleOf(conversations: readonly Conversation[], conversationId: string): string {
  const conversation = conversations.find((item) => item.id === conversationId);
  if (conversation === undefined) {
    return "unknown conversation";
  }
  return conversation.title;
}

/** Global search: matching bots (local, instant) above matching messages. */
export default function SearchOverlay(props: SearchOverlayProps): ReactElement {
  const { client, conversations, bots, projects, connected, initialQuery } = props;
  const { onOpenBot, onOpenConversation, onClose } = props;
  const [query, setQuery] = useState(initialQuery);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);

  const trimmed = query.trim();
  const { results, error, searching } = useMessageSearch(client, trimmed, connected);
  const botMatches = useMemo(() => rankBots(bots, projects, trimmed), [bots, projects, trimmed]);

  const items = useMemo(
    (): readonly SearchItem[] => [
      ...botMatches.map((match): SearchItem => ({ kind: "bot", match })),
      ...(results ?? []).map((message): SearchItem => ({ kind: "message", message })),
    ],
    [botMatches, results],
  );

  const open = (item: SearchItem): void => {
    onClose();
    if (item.kind === "bot") {
      onOpenBot(item.match.bot.id);
      return;
    }
    onOpenConversation(item.message.conversation_id);
  };

  const nav = useListKeyboardNav<HTMLInputElement, SearchItem>({
    items,
    onChoose: open,
    onEscape: onClose,
  });
  const { activeIndex, setActiveIndex } = nav;

  useScrollActiveIntoView(listRef, activeIndex);

  useEffect(() => {
    const input = inputRef.current;
    input?.focus();
    input?.select();
  }, []);

  return (
    <OverlayShell label="Search" onClose={onClose}>
      <input
        ref={inputRef}
        className="overlay-input"
        placeholder={
          connected ? "Search bots and messages…" : "Search bots… (messages unavailable offline)"
        }
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setActiveIndex(0);
        }}
        onKeyDown={nav.onKeyDown}
      />
      {items.length > 0 ? (
        <ul ref={listRef} className="overlay-list">
          {items.map((item, index) => (
            <li key={keyOf(item)}>
              {startsGroup(items, index) ? (
                <span className="overlay-section">{item.kind === "bot" ? "Bots" : "Messages"}</span>
              ) : null}
              <SearchItemRow
                item={item}
                conversations={conversations}
                active={index === activeIndex}
                onHover={() => {
                  setActiveIndex(index);
                }}
                onOpen={() => {
                  open(item);
                }}
              />
            </li>
          ))}
        </ul>
      ) : null}
      <MessageStatus
        error={error}
        searching={searching}
        results={results}
        query={trimmed}
        hasBots={botMatches.length > 0}
        connected={connected}
      />
    </OverlayShell>
  );
}

function keyOf(item: SearchItem): string {
  return item.kind === "bot" ? `bot-${item.match.bot.id}` : `msg-${item.message.id}`;
}

/** True when `index` is the first row of its kind, which gets the group label. */
function startsGroup(items: readonly SearchItem[], index: number): boolean {
  return items[index - 1]?.kind !== items[index]?.kind;
}

interface SearchItemRowProps {
  readonly item: SearchItem;
  readonly conversations: readonly Conversation[];
  readonly active: boolean;
  readonly onHover: () => void;
  readonly onOpen: () => void;
}

function SearchItemRow({
  item,
  conversations,
  active,
  onHover,
  onOpen,
}: SearchItemRowProps): ReactElement {
  if (item.kind === "bot") {
    return (
      <BotResultRow
        bot={item.match.bot}
        projectName={item.match.projectName}
        active={active}
        onHover={onHover}
        onOpen={onOpen}
      />
    );
  }
  return (
    <SearchResultRow
      message={item.message}
      conversationTitle={titleOf(conversations, item.message.conversation_id)}
      active={active}
      onHover={onHover}
      onOpen={onOpen}
    />
  );
}

interface MessageStatusProps {
  readonly error: string | null;
  readonly searching: boolean;
  readonly results: readonly BusMessage[] | null;
  readonly query: string;
  readonly hasBots: boolean;
  readonly connected: boolean;
}

/**
 * The message half of the results, which lags behind the bot half: bots match
 * locally while the daemon is still searching, so this reports on its own.
 */
function MessageStatus(props: MessageStatusProps): ReactElement | null {
  const { error, searching, results, query, hasBots, connected } = props;
  const className = hasBots ? "muted overlay-note" : "muted overlay-empty";
  if (!connected && query.length > 0) {
    return <div className={className}>Message search unavailable while disconnected.</div>;
  }
  if (error !== null) {
    return <div className={className}>Search failed: {error}</div>;
  }
  if (searching) {
    return <div className={className}>Searching…</div>;
  }
  if (results === null || results.length > 0) {
    return null;
  }
  return <div className={className}>No messages match “{query}”.</div>;
}
