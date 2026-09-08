// The global show/hide shortcut: recorded from a key event in Settings,
// stored in prefs as the accelerator string the Tauri plugin parses
// (`Super+Shift+KeyG`), and registered with the shell at startup.

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

const MODIFIER_CODES = new Set([
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
]);

// A registered global shortcut swallows the keys in every app, so combinations
// the OS needs are refused: keys that drive app switching and force quit, and
// anything a single modifier away from an everyday command like copy or quit.
const RESERVED_CODES = new Set(["Escape", "Tab"]);
const RESERVED_SHORTCUTS = new Set(["Super+Shift+KeyQ"]);
const MIN_MODIFIERS = 2;

/** An example combination, for the hint shown when a press is refused. */
export const EXAMPLE_SHORTCUT = "Super+Shift+KeyG";

/** The keys of a keyboard event the recorder cares about. */
export interface ShortcutKeys {
  readonly code: string;
  readonly metaKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
}

/**
 * What a key press means to the recorder: a usable accelerator, a press that
 * is still in progress (a lone modifier), or one that cannot be bound.
 */
export type ShortcutRecording =
  | { readonly ok: true; readonly shortcut: string }
  | { readonly ok: false; readonly reason: "pending" | "refused" };

const PENDING: ShortcutRecording = { ok: false, reason: "pending" };
const REFUSED: ShortcutRecording = { ok: false, reason: "refused" };

/**
 * Builds an accelerator from a key press. A lone modifier is still pending;
 * anything else needs at least two modifiers, one of them Cmd/Ctrl/Alt, and
 * must stay clear of the combinations the OS reserves.
 */
export function shortcutFromKeys(keys: ShortcutKeys): ShortcutRecording {
  if (keys.code.length === 0 || MODIFIER_CODES.has(keys.code)) {
    return PENDING;
  }
  if (RESERVED_CODES.has(keys.code)) {
    return REFUSED;
  }
  if (!keys.metaKey && !keys.ctrlKey && !keys.altKey) {
    return REFUSED;
  }
  const parts: string[] = [];
  if (keys.metaKey) {
    parts.push("Super");
  }
  if (keys.ctrlKey) {
    parts.push("Ctrl");
  }
  if (keys.altKey) {
    parts.push("Alt");
  }
  if (keys.shiftKey) {
    parts.push("Shift");
  }
  if (parts.length < MIN_MODIFIERS) {
    return REFUSED;
  }
  parts.push(keys.code);
  const shortcut = parts.join("+");
  return RESERVED_SHORTCUTS.has(shortcut) ? REFUSED : { ok: true, shortcut };
}

/** `navigator.platform` is deprecated, so prefer the UA-data hint. */
function platformHint(): string {
  if (typeof navigator === "undefined") {
    return "";
  }
  const data: unknown = Reflect.get(navigator, "userAgentData");
  if (typeof data === "object" && data !== null) {
    const platform: unknown = Reflect.get(data, "platform");
    if (typeof platform === "string") {
      return platform;
    }
  }
  return navigator.userAgent;
}

const MAC_LABELS: Readonly<Record<string, string>> = {
  Super: "⌘",
  Ctrl: "⌃",
  Alt: "⌥",
  Shift: "⇧",
};

const OTHER_LABELS: Readonly<Record<string, string>> = {
  Super: "Win",
  Ctrl: "Ctrl",
  Alt: "Alt",
  Shift: "Shift",
};

function keyLabel(code: string): string {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter !== null) {
    return letter[1] ?? code;
  }
  const digit = /^Digit(\d)$/.exec(code);
  if (digit !== null) {
    return digit[1] ?? code;
  }
  return code.replace(/^Arrow/, "").replace(/^Numpad/, "Num ");
}

/** A human-readable rendering of an accelerator, e.g. `⌘⇧G` or `Ctrl+Shift+G`. */
export function formatShortcut(shortcut: string): string {
  const parts = shortcut.split("+").filter((part) => part.length > 0);
  if (parts.length === 0) {
    return "";
  }
  const mac = /mac/i.test(platformHint());
  const labels = mac ? MAC_LABELS : OTHER_LABELS;
  const rendered = parts.map((part) => labels[part] ?? keyLabel(part));
  return mac ? rendered.join("") : rendered.join("+");
}

/**
 * Registers `shortcut` with the shell, replacing whatever was registered
 * before; an empty string just clears it. Rejects when the OS refuses the
 * combination, typically because another app already owns it, leaving the
 * previous shortcut in place. Does nothing in a plain browser, which has no
 * global shortcuts.
 */
export async function applyToggleWindowShortcut(shortcut: string): Promise<void> {
  if (!isTauri()) {
    return;
  }
  await invoke("set_toggle_window_shortcut", { shortcut });
}
