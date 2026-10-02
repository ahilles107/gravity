import { Mic, Paperclip, Square } from "lucide-react";
import { useRef } from "react";
import type { ReactElement } from "react";
import type { Attachments } from "./useAttachments";
import type { DictationState } from "./useDictation";

/** The draft's attachments, with how each upload is going. */
export function AttachmentChips({
  attachments,
}: {
  readonly attachments: Attachments;
}): ReactElement | null {
  if (attachments.list.length === 0) {
    return null;
  }
  return (
    <div className="chat-attachments">
      {attachments.list.map((a) => (
        <span
          key={a.key}
          className={`chat-file-chip${a.error === undefined ? "" : " chat-file-chip-error"}`}
          title={a.error ?? a.path ?? ""}
        >
          {a.name}
          {a.path === null && a.error === undefined ? " · uploading…" : ""}
          {a.error === undefined ? "" : " · failed"}
          <button
            type="button"
            className="chat-file-chip-remove"
            aria-label={`Remove ${a.name}`}
            onClick={() => {
              attachments.remove(a.key);
            }}
          >
            ×
          </button>
        </span>
      ))}
    </div>
  );
}

/** The paperclip, and the hidden file picker it opens. */
export function AttachButton({
  attachments,
  disabled,
}: {
  readonly attachments: Attachments;
  readonly disabled: boolean;
}): ReactElement {
  const picker = useRef<HTMLInputElement | null>(null);
  return (
    <>
      <button
        type="button"
        className="icon-btn chat-attach"
        aria-label="Attach files"
        title="Attach files (or paste them)"
        disabled={disabled}
        onClick={() => picker.current?.click()}
      >
        <Paperclip size={16} aria-hidden="true" />
      </button>
      <input
        ref={picker}
        type="file"
        multiple
        hidden
        aria-label="Files to attach"
        onChange={(event) => {
          attachments.attach([...(event.target.files ?? [])]);
          event.target.value = "";
        }}
      />
    </>
  );
}

/** Starts and stops dictation; red while listening. */
export function MicButton({
  voice,
  disabled,
}: {
  readonly voice: DictationState;
  readonly disabled: boolean;
}): ReactElement {
  return (
    <button
      type="button"
      className={`icon-btn chat-mic${voice.listening ? " chat-mic-on" : ""}`}
      aria-label={voice.listening ? "Stop dictation" : "Dictate"}
      aria-pressed={voice.listening}
      title={voice.listening ? "Stop dictation" : "Dictate (on this device)"}
      disabled={disabled}
      onClick={voice.toggle}
    >
      {voice.listening ? (
        <Square size={14} aria-hidden="true" />
      ) : (
        <Mic size={16} aria-hidden="true" />
      )}
    </button>
  );
}
