import type { ReactElement } from "react";

export type BotTab = "chat" | "terminal" | "browser" | "routines";

const TAB_LABEL: Readonly<Record<BotTab, string>> = {
  chat: "Chat",
  terminal: "Terminal",
  browser: "Browser",
  routines: "Routines",
};

/**
 * The tabs a bot offers. Chat and the bot's own browser need a daemon that
 * serves them, and a linked bot has only chat: its browser is on its machine.
 */
export function botTabs(chat: boolean, linked: boolean, browser = false): readonly BotTab[] {
  if (linked) {
    return ["chat"];
  }
  const tabs: BotTab[] = chat ? ["chat", "terminal"] : ["terminal"];
  if (browser) {
    tabs.push("browser");
  }
  tabs.push("routines");
  return tabs;
}

interface BotTabsProps {
  readonly tabs: readonly BotTab[];
  readonly active: BotTab;
  readonly onSelect: (tab: BotTab) => void;
  /** The bot's browser is open: its tab shows a live dot. */
  readonly browserLive?: boolean;
}

export default function BotTabs({
  tabs,
  active,
  onSelect,
  browserLive = false,
}: BotTabsProps): ReactElement {
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
          {tab === "browser" && browserLive ? (
            <span className="tab-live" role="img" aria-label="live" />
          ) : null}
        </button>
      ))}
    </nav>
  );
}
