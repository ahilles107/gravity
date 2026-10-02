import { useCallback, useEffect, useRef, useState } from "react";
import type { Dictation, DictationEvent } from "../../dictation";
import { errText } from "../../util";

export interface DictationState {
  /** Whether the platform can dictate at all; the mic hides otherwise. */
  readonly available: boolean;
  readonly listening: boolean;
  readonly error: string | null;
  readonly toggle: () => void;
}

/** `text` spoken after what was already typed, with a space between. */
export function appendSpoken(typed: string, spoken: string): string {
  if (spoken === "") {
    return typed;
  }
  return typed === "" || /\s$/.test(typed) ? `${typed}${spoken}` : `${typed} ${spoken}`;
}

/**
 * Dictation into a draft: what is said appears live after what was typed,
 * and stays when listening stops.
 */
export function useDictation(
  dictation: Dictation | undefined,
  draft: string,
  setDraft: (text: string) => void,
): DictationState {
  const [available, setAvailable] = useState(false);
  const [listening, setListening] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const typed = useRef("");
  const stop = useRef<(() => void) | null>(null);

  useEffect(() => {
    let live = true;
    if (dictation !== undefined) {
      void dictation.available().then((yes) => {
        if (live) {
          setAvailable(yes);
        }
        return yes;
      });
    }
    return () => {
      live = false;
      stop.current?.();
    };
  }, [dictation]);

  const onEvent = useCallback(
    (event: DictationEvent): void => {
      if (event.kind === "partial" || event.kind === "final") {
        setDraft(appendSpoken(typed.current, event.text));
      }
      if (event.kind === "final" || event.kind === "ended") {
        setListening(false);
        stop.current = null;
        if (event.kind === "ended" && event.error !== undefined) {
          setError(event.error);
        }
      }
    },
    [setDraft],
  );

  const toggle = useCallback((): void => {
    if (dictation === undefined) {
      return;
    }
    if (stop.current !== null) {
      stop.current();
      stop.current = null;
      setListening(false);
      return;
    }
    typed.current = draft;
    setError(null);
    setListening(true);
    dictation
      .start(onEvent)
      .then((stopper) => {
        stop.current = stopper;
        return stopper;
      })
      .catch((failure: unknown) => {
        setListening(false);
        setError(errText(failure));
      });
  }, [dictation, draft, onEvent]);

  return { available, listening, error, toggle };
}
