import { useRef, useState } from "react";
import type { ClipboardEvent, KeyboardEvent, ReactElement } from "react";
import type { Dictation } from "../../dictation";
import { AttachButton, AttachmentChips, MicButton } from "./ComposerParts";
import { useAttachments, withAttachments } from "./useAttachments";
import { useDictation } from "./useDictation";

interface ChatComposerProps {
  /** Why the owner cannot write, or null when they can. */
  readonly disabledReason: string | null;
  readonly placeholder: string;
  readonly onSend: (text: string) => Promise<void>;
  /** Uploads an attached file and returns where bots can read it; absent to turn attaching off. */
  readonly onAttach?: (file: File) => Promise<string>;
  /** Shown under a draft that starts with `/`, or null when slash commands are not special. */
  readonly slashHint?: string | null;
  /** On-device dictation; the mic button shows only where it is available. */
  readonly dictation?: Dictation;
}

const LISTENING = "Listening… press the stop button when you are done.";

/** Enter sends, Shift+Enter adds a line. A failed send keeps the draft. */
export default function ChatComposer(props: ChatComposerProps): ReactElement {
  const { disabledReason, onSend, onAttach } = props;
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const field = useRef<HTMLTextAreaElement | null>(null);
  const disabled = disabledReason !== null;
  const attachments = useAttachments(onAttach);
  const voice = useDictation(props.dictation, draft, setDraft);
  const empty = draft.trim() === "" && attachments.ready.length === 0;
  const blocked = disabled || sending || attachments.uploading || empty;

  const send = async (): Promise<void> => {
    if (blocked) {
      return;
    }
    setSending(true);
    try {
      await onSend(withAttachments(draft.trim(), attachments.ready));
      setDraft("");
      attachments.clear();
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

  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>): void => {
    const files = [...event.clipboardData.files];
    if (files.length > 0 && onAttach !== undefined) {
      event.preventDefault();
      attachments.attach(files);
    }
  };

  const slash =
    props.slashHint != null && draft.trimStart().startsWith("/") ? props.slashHint : null;
  const hint = voice.error ?? (voice.listening ? LISTENING : slash);
  return (
    <div className="chat-composer-wrap">
      <AttachmentChips attachments={attachments} />
      {hint === null ? null : <div className="chat-composer-hint">{hint}</div>}
      <div className="chat-composer">
        {onAttach === undefined ? null : (
          <AttachButton attachments={attachments} disabled={disabled} />
        )}
        {voice.available ? <MicButton voice={voice} disabled={disabled} /> : null}
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
          onPaste={onPaste}
        />
        <button
          type="button"
          className="btn btn-primary chat-composer-send"
          disabled={blocked}
          onClick={() => void send()}
        >
          Send
        </button>
      </div>
    </div>
  );
}
