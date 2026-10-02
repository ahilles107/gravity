import { useEffect } from "react";
import type { BotTab } from "./BotTabs";

/**
 * ⌘1…⌘3 switch the bot's tabs and ⌘L jumps to the chat composer. Registered
 * in the capture phase so a focused terminal cannot swallow them.
 */
export function useBotKeys(tabs: readonly BotTab[], onSelect: (tab: BotTab) => void): void {
  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) {
        return;
      }
      const index = Number.parseInt(event.key, 10);
      const tab = Number.isNaN(index) ? undefined : tabs[index - 1];
      if (tab !== undefined) {
        event.preventDefault();
        onSelect(tab);
        return;
      }
      if (event.key.toLowerCase() === "l" && tabs.includes("chat")) {
        event.preventDefault();
        onSelect("chat");
        window.requestAnimationFrame(() => {
          document.querySelector<HTMLTextAreaElement>("[data-chat-composer]")?.focus();
        });
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
    };
  }, [tabs, onSelect]);
}
