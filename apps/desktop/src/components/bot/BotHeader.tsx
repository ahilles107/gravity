import type { ReactElement } from "react";
import type { Bot, BotState } from "../../protocol/entities";
import BotAvatar from "../BotAvatar";
import { InfoPanelIcon } from "./icons";

const STATE_LABEL: Readonly<Record<BotState, string>> = {
  starting: "starting",
  ready: "ready",
  working: "working",
  waiting_for_user: "waiting for you",
  waiting_for_approval: "waiting for approval",
  rate_limited: "rate limited",
  auth_failed: "auth failed",
  crashed: "crashed",
  stopping: "stopping",
  stopped: "stopped",
};

/* States the dot alone explains; anything else — a new one included — still
   earns words in the header. */
const QUIET_STATES: ReadonlySet<BotState> = new Set<BotState>([
  "starting",
  "ready",
  "working",
  "stopping",
  "stopped",
]);

interface BotHeaderProps {
  readonly bot: Bot;
  readonly canControl: boolean;
  readonly infoPanelCollapsed: boolean;
  readonly onToggleInfoPanel: () => void;
}

export default function BotHeader(props: BotHeaderProps): ReactElement {
  const { bot, canControl, infoPanelCollapsed, onToggleInfoPanel } = props;
  const infoPanelAction = infoPanelCollapsed ? "Expand bot info" : "Collapse bot info";
  const stateLabel = STATE_LABEL[bot.state];
  const stateTitle = bot.state_reason ? `${stateLabel} — ${bot.state_reason}` : stateLabel;
  return (
    <header className="view-header" data-tauri-drag-region="deep">
      <div className="view-header-main">
        <span className={`dot dot-${bot.state}`} title={stateTitle} aria-label={stateTitle} />
        <BotAvatar avatar={bot.avatar} name={bot.name} id={bot.id} size="md" />
        <h2 className="view-title">{bot.name}</h2>
        {QUIET_STATES.has(bot.state) ? null : (
          <span className="state-chip" title={stateTitle}>
            {stateLabel}
          </span>
        )}
      </div>
      <div className="view-header-actions">
        {/* Bots are always-on, so there is no lifecycle control here. The
            terminal is the user's whenever they hold `control`; only
            read-only connections need a badge. */}
        {canControl ? null : <span className="readonly-badge">Read-only</span>}
        <button
          type="button"
          className="info-panel-toggle"
          aria-label={infoPanelAction}
          aria-expanded={!infoPanelCollapsed}
          aria-controls="bot-info-panel"
          title={infoPanelAction}
          onClick={onToggleInfoPanel}
        >
          <InfoPanelIcon collapsed={infoPanelCollapsed} />
        </button>
      </div>
    </header>
  );
}
