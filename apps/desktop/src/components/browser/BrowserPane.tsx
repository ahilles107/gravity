import type { ReactElement } from "react";
import type { BrowserFramePush, BrowserTabsPush } from "../../protocol/agents";
import type { DaemonApi } from "../../protocol/api";
import type { Bot } from "../../protocol/entities";
import BrowserActivityList from "./BrowserActivityList";
import { useBrowserActivity } from "./useBrowserActivity";
import type { BrowserWatch } from "./useBrowserWatch";

interface BrowserPaneProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  /** The bot's browser as it streams (see `useBrowserWatch`). */
  readonly watch: BrowserWatch;
  readonly connected: boolean;
}

/** Where the bot's own browser stands, said above the screen. */
function chromeNote(bot: Bot): string {
  return bot.user_chrome === true
    ? `${bot.name} uses a browser of its own, and may also use your Chrome.`
    : `${bot.name} uses a browser of its own; your Chrome is off limits.`;
}

/** Each of the bot's tabs; picking one shows it, "Follow" goes back to the bot's. */
export function TabStrip({
  tabs,
  onPick,
}: {
  readonly tabs: BrowserTabsPush;
  readonly onPick: (tabId: string | null) => void;
}): ReactElement {
  return (
    <div className="browser-tabs" role="tablist" aria-label="Browser tabs">
      {tabs.tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          aria-selected={tab.id === tabs.active}
          className={`browser-tab ${tab.id === tabs.active ? "browser-tab-active" : ""}`}
          title={tab.url}
          onClick={() => {
            onPick(tab.id);
          }}
        >
          {tab.title === "" ? tab.url : tab.title}
        </button>
      ))}
      <button
        type="button"
        className={`browser-follow ${tabs.following === false ? "" : "browser-follow-on"}`}
        aria-pressed={tabs.following !== false}
        title="Show whichever tab the bot is using"
        onClick={() => {
          onPick(null);
        }}
      >
        Follow bot
      </button>
    </div>
  );
}

/** The tab on show, as its latest screen. */
export function BrowserScreen({
  tabs,
  frame,
  name,
}: {
  readonly tabs: BrowserTabsPush | null;
  readonly frame: BrowserFramePush | null;
  readonly name: string;
}): ReactElement {
  if (tabs === null) {
    return <div className="browser-empty muted">Looking for the browser…</div>;
  }
  if (!tabs.open) {
    return (
      <div className="browser-empty muted">
        {tabs.reason ?? `${name}'s browser is closed. It opens when ${name} first browses.`}
      </div>
    );
  }
  const url = tabs.tabs.find((tab) => tab.id === tabs.active)?.url ?? "";
  return (
    <div className="browser-screen">
      <div className="browser-url" title={url}>
        {url}
      </div>
      {frame !== null && frame.tab_id === tabs.active ? (
        <img
          className="browser-frame"
          src={`data:image/jpeg;base64,${frame.data}`}
          width={frame.width}
          height={frame.height}
          alt={`What ${name}'s browser shows`}
        />
      ) : (
        <div className="browser-empty muted">Waiting for the page…</div>
      )}
    </div>
  );
}

/**
 * The bot's own browser, live: its tabs, the page on show, and every
 * browser action it took with the turn that led to it.
 */
export default function BrowserPane({
  client,
  bot,
  watch,
  connected,
}: BrowserPaneProps): ReactElement {
  const activity = useBrowserActivity(client, bot.id, connected);
  return (
    <div className="browser-pane">
      <div className="browser-main">
        <div className="browser-note muted">{chromeNote(bot)}</div>
        {watch.tabs !== null && watch.tabs.open ? (
          <TabStrip tabs={watch.tabs} onPick={watch.pick} />
        ) : null}
        {watch.error === null ? null : <div className="chat-note chat-error">{watch.error}</div>}
        <BrowserScreen tabs={watch.tabs} frame={watch.frame} name={bot.name} />
      </div>
      <BrowserActivityList activity={activity.items} error={activity.error} />
    </div>
  );
}
