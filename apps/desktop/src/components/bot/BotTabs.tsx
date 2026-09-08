import type { ReactElement } from "react";

const BOT_TABS = ["terminal", "routines"] as const;
export type BotTab = (typeof BOT_TABS)[number];

const TAB_LABEL: Readonly<Record<BotTab, string>> = {
  terminal: "Terminal",
  routines: "Routines",
};

interface BotTabsProps {
  readonly active: BotTab;
  readonly onSelect: (tab: BotTab) => void;
}

export default function BotTabs({ active, onSelect }: BotTabsProps): ReactElement {
  return (
    <nav className="tabs">
      {BOT_TABS.map((tab) => (
        <button
          key={tab}
          type="button"
          className={`tab ${tab === active ? "tab-active" : ""}`}
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
