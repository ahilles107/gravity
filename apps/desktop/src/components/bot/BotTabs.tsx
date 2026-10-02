import type { ReactElement } from "react";

export type BotTab = "chat" | "terminal" | "browser" | "routines";

const TAB_LABEL: Readonly<Record<BotTab, string>> = {
  chat: "Chat",
  terminal: "Terminal",
  browser: "Browser",
  routines: "Routines",
};

/** What the daemon serves, which decides the tabs a bot offers. */
export interface TabSupport {
  readonly chat: boolean;
  /** Each bot's own browser, watched live. */
  readonly browser?: boolean;
  /** A linked bot's terminal, relayed from its machine. */
  readonly peerTerminal?: boolean;
  /** A linked bot's browser, relayed from its machine. */
  readonly peerBrowser?: boolean;
}

/**
 * The tabs a bot offers. A linked bot has its chat, and its terminal and
 * browser when the daemon relays them from its machine.
 */
export function botTabs(support: TabSupport, linked: boolean): readonly BotTab[] {
  const { chat, browser = false, peerTerminal = false, peerBrowser = false } = support;
  if (linked) {
    const tabs: BotTab[] = ["chat"];
    if (peerTerminal) {
      tabs.push("terminal");
    }
    if (peerBrowser) {
      tabs.push("browser");
    }
    return tabs;
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
