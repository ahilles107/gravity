import { Paperclip } from "lucide-react";
import { useRef, useState } from "react";
import type { ClipboardEvent, KeyboardEvent, ReactElement } from "react";
import { errText } from "../../util";

interface ChatComposerProps {
  /** Why the owner cannot write, or null when they can. */
  readonly disabledReason: string | null;
  readonly placeholder: string;
  readonly onSend: (text: string) => Promise<void>;
  /** Uploads an attached file and returns where bots can read it; absent to turn attaching off. */
  readonly onAttach?: (file: File) => Promise<string>;
  /** Shown under a draft that starts with `/`, or null when slash commands are not special. */
  readonly slashHint?: string | null;
}

interface Attachment {
  readonly key: number;
  readonly name: string;
  /** Null while it uploads. */
  readonly path: string | null;
  readonly error?: string;
}

/** The message text with the attached files' paths appended. */
export function withAttachments(text: string, attachments: readonly Attachment[]): string {
  const paths = attachments.flatMap((a) => (a.path === null ? [] : [a.path]));
  if (paths.length === 0) {
    return text;
  }
  const listing = paths.map((path) => `- ${path}`).join("\n");
  return text === "" ? `Attached files:\n${listing}` : `${text}\n\nAttached files:\n${listing}`;
}

/** Enter sends, Shift+Enter adds a line. A failed send keeps the draft. */
export default function ChatComposer(props: ChatComposerProps): ReactElement {
  const { disabledReason, onSend, onAttach } = props;
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [attachments, setAttachments] = useState<readonly Attachment[]>([]);
  const field = useRef<HTMLTextAreaElement | null>(null);
  const picker = useRef<HTMLInputElement | null>(null);
  const nextKey = useRef(0);
  const disabled = disabledReason !== null;
  const uploading = attachments.some((a) => a.path === null && a.error === undefined);
  const ready = attachments.filter((a) => a.path !== null);

  const attach = (files: readonly File[]): void => {
    if (onAttach === undefined) {
      return;
    }
    for (const file of files) {
      const key = nextKey.current++;
      setAttachments((current) => [...current, { key, name: file.name, path: null }]);
      void onAttach(file)
        .then((path) => {
          setAttachments((current) => current.map((a) => (a.key === key ? { ...a, path } : a)));
          return path;
        })
        .catch((failure: unknown) => {
          const error = errText(failure);
          setAttachments((current) => current.map((a) => (a.key === key ? { ...a, error } : a)));
        });
    }
  };

  const send = async (): Promise<void> => {
    const text = draft.trim();
    if ((text === "" && ready.length === 0) || disabled || sending || uploading) {
      return;
    }
    setSending(true);
    try {
      await onSend(withAttachments(text, ready));
      setDraft("");
      setAttachments([]);
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
      attach(files);
    }
  };

  const hint =
    props.slashHint != null && draft.trimStart().startsWith("/") ? props.slashHint : null;
  return (
    <div className="chat-composer-wrap">
      {attachments.length === 0 ? null : (
        <div className="chat-attachments">
          {attachments.map((a) => (
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
                  setAttachments((current) => current.filter((other) => other.key !== a.key));
                }}
              >
                ×
              </button>
            </span>
          ))}
        </div>
      )}
      {hint === null ? null : <div className="chat-composer-hint">{hint}</div>}
      <div className="chat-composer">
        {onAttach === undefined ? null : (
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
                attach([...(event.target.files ?? [])]);
                event.target.value = "";
              }}
            />
          </>
        )}
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
          disabled={disabled || sending || uploading || (draft.trim() === "" && ready.length === 0)}
          onClick={() => void send()}
        >
          Send
        </button>
      </div>
    </div>
  );
}
