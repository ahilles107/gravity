import { useRef, useState } from "react";
import type { KeyboardEvent, ReactElement } from "react";

interface ChatComposerProps {
  /** Why the owner cannot write, or null when they can. */
  readonly disabledReason: string | null;
  readonly placeholder: string;
  readonly onSend: (text: string) => Promise<void>;
}

/** Enter sends, Shift+Enter adds a line. A failed send keeps the draft. */
export default function ChatComposer(props: ChatComposerProps): ReactElement {
  const { disabledReason, onSend } = props;
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const field = useRef<HTMLTextAreaElement | null>(null);
  const disabled = disabledReason !== null;

  const send = async (): Promise<void> => {
    const text = draft.trim();
    if (text === "" || disabled || sending) {
      return;
    }
    setSending(true);
    try {
      await onSend(text);
      setDraft("");
    } finally {
      setSending(false);
      field.current?.focus();
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>): void => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      void send();
    }
  };

  return (
    <div className="chat-composer">
      <textarea
        ref={field}
        className="chat-composer-field"
        aria-label="Message"
        data-chat-composer=""
        rows={1}
        value={draft}
        disabled={disabled}
        placeholder={disabledReason ?? props.placeholder}
        onChange={(event) => {
          setDraft(event.target.value);
        }}
        onKeyDown={onKeyDown}
      />
      <button
        type="button"
        className="btn btn-primary chat-composer-send"
        disabled={disabled || sending || draft.trim() === ""}
        onClick={() => void send()}
      >
        Send
      </button>
    </div>
  );
}
