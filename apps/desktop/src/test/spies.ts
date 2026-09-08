import { vi } from "vitest";
import type { AddToast } from "../app/useToasts";
import type { Bot, NotifyLevel, Routine } from "../protocol/entities";

/** `onToast` as most panels declare it. */
export function toastSpy() {
  return vi.fn<(level: NotifyLevel, title: string, body: string) => void>();
}

/** `onToast` for views that may attach an action (BotView and below). */
export function actionToastSpy() {
  return vi.fn<AddToast>();
}

export function botSpy() {
  return vi.fn<(bot: Bot) => void>();
}

export function routinesSpy() {
  return vi.fn<(botId: string, routines: readonly Routine[]) => void>();
}

/**
 * An in-memory `localStorage`. jsdom's own is not usable here, and view
 * preferences (pinned bots, endpoint) read and write it.
 */
export function stubLocalStorage(
  initial: Readonly<Record<string, string>> = {},
): Map<string, string> {
  const store = new Map(Object.entries(initial));
  vi.stubGlobal("localStorage", {
    getItem: (key: string): string | null => store.get(key) ?? null,
    setItem: (key: string, value: string): void => {
      store.set(key, value);
    },
  });
  return store;
}

export function voidSpy() {
  return vi.fn<() => void>();
}
