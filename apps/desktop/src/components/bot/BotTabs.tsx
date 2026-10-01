import type { ReactElement } from "react";

export type BotTab = "chat" | "terminal" | "routines";

const TAB_LABEL: Readonly<Record<BotTab, string>> = {
  chat: "Chat",
  terminal: "Terminal",
  routines: "Routines",
};

/** The tabs a bot offers: chat needs a daemon that serves it, and a linked bot has only chat. */
export function botTabs(chat: boolean, linked: boolean): readonly BotTab[] {
  if (linked) {
    return ["chat"];
  }
  return chat ? ["chat", "terminal", "routines"] : ["terminal", "routines"];
}

interface BotTabsProps {
  readonly tabs: readonly BotTab[];
  readonly active: BotTab;
  readonly onSelect: (tab: BotTab) => void;
}

export default function BotTabs({ tabs, active, onSelect }: BotTabsProps): ReactElement {
  return (
    <nav className="tabs">
      {tabs.map((tab, index) => (
        <button
          key={tab}
          type="button"
          className={`tab ${tab === active ? "tab-active" : ""}`}
          title={`${TAB_LABEL[tab]} (⌘${index + 1})`}
          onClick={() => {
            onSelect(tab);
          }}
        >
          {TAB_LABEL[tab]}
        </button>
      ))}
    </nav>
  );
}
