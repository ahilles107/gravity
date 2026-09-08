import type { Story } from "@ladle/react";
import { message } from "../../test/fixtures";
import SearchResultRow from "./SearchResultRow";

const noop = (): void => {};

export const Kinds: Story = () => (
  <div style={{ maxWidth: 560 }}>
    {(["task", "reply", "done", "note", "chat"] as const).map((kind, index) => (
      <SearchResultRow
        key={kind}
        message={message({
          id: kind,
          kind,
          sender: { kind: "bot", bot_id: "b1", name: "alice" },
          body: `A ${kind} message about the weekly report and what changed since last run.`,
        })}
        conversationTitle="alice"
        active={index === 0}
        onHover={noop}
        onOpen={noop}
      />
    ))}
  </div>
);

export const LongBody: Story = () => (
  <div style={{ maxWidth: 560 }}>
    <SearchResultRow
      message={message({
        body:
          "This body is deliberately much longer than the snippet limit so the row has to " +
          "truncate it with an ellipsis instead of wrapping forever. It keeps going and going " +
          "well past one hundred and sixty characters to prove the point.",
      })}
      conversationTitle="alice"
      active={false}
      onHover={noop}
      onOpen={noop}
    />
  </div>
);
