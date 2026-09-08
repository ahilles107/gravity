import { useEffect, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent, PointerEvent, ReactElement } from "react";
import { isStopped } from "../app/bots";
import type { AddToast } from "../app/useToasts";
import type { DaemonApi } from "../protocol/api";
import type { Bot, Routine } from "../protocol/entities";
import {
  DEFAULT_BOT_INFO_PANEL,
  MIN_BOT_INFO_PANEL_WIDTH,
  loadBotInfoPanel,
  saveBotInfoPanel,
} from "../settings";
import BotHeader from "./bot/BotHeader";
import BotTabs from "./bot/BotTabs";
import type { BotTab } from "./bot/BotTabs";
import InfoPanel from "./InfoPanel";
import RoutinesPanel from "./RoutinesPanel";
import TerminalPane from "./TerminalPane";

interface BotViewProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly bots: readonly Bot[];
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onBotUpdated: (bot: Bot) => void;
  readonly onRoutinesChanged: (botId: string, routines: readonly Routine[]) => void;
  readonly onToast: AddToast;
}

interface DragState {
  readonly pointerId: number;
  readonly startWidth: number;
  readonly startX: number;
}

const FALLBACK_MAX_INFO_PANEL_WIDTH = 640;

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(Math.max(value, minimum), maximum);
}

export default function BotView(props: BotViewProps): ReactElement {
  const { client, bot, bots, connected, canControl, onToast } = props;
  const [tab, setTab] = useState<BotTab>("terminal");
  const [infoPanel, setInfoPanel] = useState(loadBotInfoPanel);
  const [maxInfoPanelWidth, setMaxInfoPanelWidth] = useState(FALLBACK_MAX_INFO_PANEL_WIDTH);
  const layoutRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<DragState | null>(null);
  // The terminal belongs to the user: any `control` connection may type while
  // the bot runs. Bus deliveries arrive via the bot's inbox socket, never here.
  const canWrite = canControl && !isStopped(bot);

  useEffect(() => {
    saveBotInfoPanel(infoPanel);
  }, [infoPanel]);

  useEffect(() => {
    const updateMaximumWidth = (): void => {
      const layoutWidth = layoutRef.current?.getBoundingClientRect().width ?? 0;
      if (layoutWidth <= 0) {
        return;
      }
      const maximum = Math.max(MIN_BOT_INFO_PANEL_WIDTH, Math.floor(layoutWidth / 2));
      setMaxInfoPanelWidth(maximum);
      setInfoPanel((current) =>
        current.width <= maximum ? current : { ...current, width: maximum },
      );
    };
    updateMaximumWidth();
    window.addEventListener("resize", updateMaximumWidth);
    return () => {
      window.removeEventListener("resize", updateMaximumWidth);
    };
  }, []);

  const setInfoPanelWidth = (width: number): void => {
    setInfoPanel((current) => ({
      ...current,
      width: clamp(width, MIN_BOT_INFO_PANEL_WIDTH, maxInfoPanelWidth),
    }));
  };

  const startResize = (event: PointerEvent<HTMLDivElement>): void => {
    dragRef.current = {
      pointerId: event.pointerId,
      startWidth: infoPanel.width,
      startX: event.clientX,
    };
    if (typeof event.currentTarget.setPointerCapture === "function") {
      event.currentTarget.setPointerCapture(event.pointerId);
    }
  };

  const resize = (event: PointerEvent<HTMLDivElement>): void => {
    const drag = dragRef.current;
    if (drag === null || drag.pointerId !== event.pointerId) {
      return;
    }
    setInfoPanelWidth(drag.startWidth + drag.startX - event.clientX);
  };

  const stopResize = (event: PointerEvent<HTMLDivElement>): void => {
    if (dragRef.current?.pointerId !== event.pointerId) {
      return;
    }
    dragRef.current = null;
    if (
      typeof event.currentTarget.hasPointerCapture === "function" &&
      event.currentTarget.hasPointerCapture(event.pointerId)
    ) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  const resizeWithKeyboard = (event: KeyboardEvent<HTMLDivElement>): void => {
    const step = event.shiftKey ? 40 : 10;
    let nextWidth: number | undefined;
    if (event.key === "ArrowLeft") {
      nextWidth = infoPanel.width + step;
    } else if (event.key === "ArrowRight") {
      nextWidth = infoPanel.width - step;
    } else if (event.key === "Home") {
      nextWidth = MIN_BOT_INFO_PANEL_WIDTH;
    } else if (event.key === "End") {
      nextWidth = maxInfoPanelWidth;
    }
    if (nextWidth !== undefined) {
      event.preventDefault();
      setInfoPanelWidth(nextWidth);
    }
  };

  const panelStyle: CSSProperties = {
    width: infoPanel.width,
    maxWidth: "50%",
  };

  return (
    <div className="bot-view">
      <BotHeader
        bot={bot}
        canControl={canControl}
        infoPanelCollapsed={infoPanel.collapsed}
        onToggleInfoPanel={() => {
          setInfoPanel((current) => ({ ...current, collapsed: !current.collapsed }));
        }}
      />
      <div className="bot-view-layout" ref={layoutRef}>
        <section className="bot-view-main">
          <BotTabs active={tab} onSelect={setTab} />
          <div className="bot-view-body">
            {/* Keep the terminal mounted across tab switches to preserve the buffer. */}
            <div className={tab === "terminal" ? "tab-pane" : "tab-pane tab-pane-hidden"}>
              <TerminalPane client={client} botId={bot.id} canWrite={canWrite} onToast={onToast} />
            </div>
            {tab === "routines" ? (
              <div className="tab-pane tab-pane-scroll">
                <RoutinesPanel
                  client={client}
                  bot={bot}
                  bots={bots}
                  connected={connected}
                  canControl={canControl}
                  onRoutinesChanged={props.onRoutinesChanged}
                  onToast={onToast}
                />
              </div>
            ) : null}
          </div>
        </section>
        {infoPanel.collapsed ? null : (
          <div
            className="bot-info-resizer"
            role="separator"
            aria-label="Resize bot info"
            aria-orientation="vertical"
            aria-valuemin={MIN_BOT_INFO_PANEL_WIDTH}
            aria-valuemax={maxInfoPanelWidth}
            aria-valuenow={Math.round(infoPanel.width)}
            tabIndex={0}
            onDoubleClick={() => {
              setInfoPanelWidth(DEFAULT_BOT_INFO_PANEL.width);
            }}
            onKeyDown={resizeWithKeyboard}
            onPointerDown={startResize}
            onPointerMove={resize}
            onPointerUp={stopResize}
            onPointerCancel={stopResize}
          />
        )}
        <aside
          id="bot-info-panel"
          className={`bot-info-panel${infoPanel.collapsed ? " bot-info-panel-collapsed" : ""}`}
          style={panelStyle}
          aria-label="Bot info"
        >
          <InfoPanel
            client={client}
            bot={bot}
            connected={connected}
            canControl={canControl}
            onBotUpdated={props.onBotUpdated}
            onToast={onToast}
          />
        </aside>
      </div>
    </div>
  );
}
