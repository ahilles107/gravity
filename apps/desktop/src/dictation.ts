import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./tauri";

/** What the native recognizer reports while a dictation runs. */
export type DictationEvent =
  | { readonly kind: "partial"; readonly text: string }
  | { readonly kind: "final"; readonly text: string }
  | { readonly kind: "ended"; readonly error?: string };

function isDictationEvent(value: unknown): value is DictationEvent {
  if (typeof value !== "object" || value === null || !("kind" in value)) {
    return false;
  }
  return value.kind === "partial" || value.kind === "final" || value.kind === "ended";
}

/** The app's on-device dictation: the shell's speech recognizer, or none in a browser. */
export interface Dictation {
  readonly available: () => Promise<boolean>;
  /** Starts listening; `onEvent` sees the transcript. Resolves to a stop function. */
  readonly start: (onEvent: (event: DictationEvent) => void) => Promise<() => void>;
}

export const nativeDictation: Dictation = {
  available: async () => {
    if (!isTauri()) {
      return false;
    }
    try {
      return await invoke<boolean>("dictation_available");
    } catch {
      return false;
    }
  },
  start: async (onEvent) => {
    const unlisten = await listen("dictation", (event) => {
      if (isDictationEvent(event.payload)) {
        onEvent(event.payload);
      }
    });
    try {
      await invoke("start_dictation");
    } catch (failure) {
      unlisten();
      throw failure instanceof Error ? failure : new Error(String(failure));
    }
    return () => {
      void invoke("stop_dictation");
      // Late events (the final transcript) still arrive until the composer is done.
      window.setTimeout(unlisten, 3000);
    };
  },
};
