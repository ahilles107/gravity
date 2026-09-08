import { useCallback, useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import { updatePrefs, usePrefs } from "../../prefs";
import {
  applyToggleWindowShortcut,
  EXAMPLE_SHORTCUT,
  formatShortcut,
  shortcutFromKeys,
} from "../../windowShortcut";

interface RecorderProps {
  readonly shortcut: string;
  readonly recording: boolean;
  readonly busy: boolean;
  readonly onStart: () => void;
  readonly onCancel: () => void;
  readonly onRecord: (shortcut: string) => void;
  readonly onRefuse: () => void;
}

function recorderLabel(shortcut: string, recording: boolean): string {
  if (recording) {
    return "Press keys…";
  }
  return shortcut.length === 0 ? "Not set" : formatShortcut(shortcut);
}

/**
 * A button that turns into a key listener on click. Escape backs out,
 * Backspace or Delete clears, and any bindable combination records. A press
 * that cannot be bound is left alone rather than swallowed, so ⌘Q still quits.
 *
 * Keys are captured on the window rather than the button: WebKit does not
 * focus a button on click, so its own keydown would never fire, and capture
 * phase wins over the terminal's textarea.
 */
function ShortcutRecorder(props: RecorderProps): ReactElement {
  const { shortcut, recording, busy, onStart, onCancel, onRecord, onRefuse } = props;
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!recording) {
      return undefined;
    }
    buttonRef.current?.focus();
    const claim = (event: KeyboardEvent): void => {
      event.preventDefault();
      event.stopPropagation();
    };
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        claim(event);
        onCancel();
        return;
      }
      if (
        (event.key === "Backspace" || event.key === "Delete") &&
        !event.metaKey &&
        !event.ctrlKey
      ) {
        claim(event);
        onRecord("");
        return;
      }
      const recorded = shortcutFromKeys(event);
      if (recorded.ok) {
        claim(event);
        onRecord(recorded.shortcut);
        return;
      }
      if (recorded.reason === "refused") {
        onRefuse();
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return (): void => {
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [recording, onCancel, onRecord, onRefuse]);

  return (
    <button
      ref={buttonRef}
      type="button"
      className={`settings-shortcut${recording ? " settings-shortcut-recording" : ""}`}
      aria-label="Show or hide window shortcut"
      disabled={busy}
      onClick={recording ? onCancel : onStart}
      onBlur={recording ? onCancel : undefined}
    >
      {recorderLabel(shortcut, recording)}
    </button>
  );
}

/** Global keyboard shortcuts; they work while another app is frontmost. */
export default function ShortcutSettings(): ReactElement {
  const prefs = usePrefs();
  const [recording, setRecording] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const example = formatShortcut(EXAMPLE_SHORTCUT);

  const startRecording = useCallback((): void => {
    setRecording(true);
    setError("");
  }, []);
  const cancelRecording = useCallback((): void => {
    setRecording(false);
  }, []);
  const refuseRecording = useCallback((): void => {
    setError(`That combination is not available. Use two or more modifiers, such as ${example}.`);
  }, [example]);

  const commit = useCallback((shortcut: string): void => {
    setRecording(false);
    setBusy(true);
    setError("");
    const apply = async (): Promise<void> => {
      try {
        await applyToggleWindowShortcut(shortcut);
        updatePrefs({ toggleWindowShortcut: shortcut });
      } catch (cause) {
        const detail = cause instanceof Error ? cause.message : String(cause);
        setError(`${formatShortcut(shortcut)} could not be registered: ${detail}`);
      } finally {
        setBusy(false);
      }
    };
    void apply();
  }, []);

  return (
    <div className="settings-section">
      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Show or hide Gravity</div>
          <div className="settings-row-help">
            Works from any app. Click the field, then press a combination of two or more modifiers
            and a key, such as {example}. Backspace clears it.
          </div>
          {error.length > 0 ? <div className="settings-row-error">{error}</div> : null}
        </div>
        <div className="settings-row-control">
          <ShortcutRecorder
            shortcut={prefs.toggleWindowShortcut}
            recording={recording}
            busy={busy}
            onStart={startRecording}
            onCancel={cancelRecording}
            onRecord={commit}
            onRefuse={refuseRecording}
          />
          {prefs.toggleWindowShortcut.length > 0 && !recording ? (
            <button
              type="button"
              className="btn btn-small"
              disabled={busy}
              onClick={() => {
                commit("");
              }}
            >
              Clear
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}
