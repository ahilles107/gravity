import { useCallback, useEffect, useState } from "react";
import type { BrowserFramePush, BrowserTabsPush } from "../../protocol/agents";
import type { DaemonApi } from "../../protocol/api";
import { errText } from "../../util";

export interface BrowserWatch {
  /** The bot's tabs, or null until the daemon has said. */
  readonly tabs: BrowserTabsPush | null;
  /** The newest screen of the tab on show. */
  readonly frame: BrowserFramePush | null;
  readonly error: string | null;
  /** Shows another tab, or follows the bot again with null. */
  readonly pick: (tabId: string | null) => void;
}

/**
 * Streams a bot's browser while `active`: the daemon pushes its tabs as they
 * change and each new screen of the tab on show. The bot view holds it for as
 * long as the bot is selected, so the browser is live whichever tab is open.
 * Mount it keyed by bot: a different bot's browser starts from nothing.
 */
export function useBrowserWatch(
  client: DaemonApi,
  botId: string,
  active: boolean,
  connected: boolean,
): BrowserWatch {
  const [tabs, setTabs] = useState<BrowserTabsPush | null>(null);
  const [frame, setFrame] = useState<BrowserFramePush | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);

  useEffect(() => {
    if (!active || !connected) {
      return undefined;
    }
    const offTabs = client.on("browser_tabs", (push) => {
      if (push.bot_id === botId) {
        setTabs(push);
      }
    });
    const offFrame = client.on("browser_frame", (push) => {
      if (push.bot_id === botId) {
        setFrame(push);
      }
    });
    const watch = async (): Promise<void> => {
      try {
        await client.request(
          picked === null
            ? { type: "watch_browser", bot_id: botId }
            : { type: "watch_browser", bot_id: botId, tab_id: picked },
          "ok",
        );
        setError(null);
      } catch (failure) {
        setError(errText(failure));
      }
    };
    void watch();
    return () => {
      offTabs();
      offFrame();
      client.request({ type: "unwatch_browser" }, "ok").catch(() => undefined);
    };
  }, [client, botId, active, connected, picked]);

  const pick = useCallback((tabId: string | null): void => {
    setPicked(tabId);
  }, []);

  return { tabs, frame, error, pick };
}
