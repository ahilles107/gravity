import type { Selection } from "../../app/selection";
import type { BotActivity } from "../../protocol/entities";

/** Per-row badge inputs shared by the sidebar and each project section. */
export interface SidebarTreeProps {
  readonly unreadBots: Readonly<Record<string, number>>;
  readonly failedByBot: ReadonlyMap<string, number>;
  readonly nextRun: Readonly<Record<string, string>>;
  /** Pinned bot ids in display order; those bots move to the pinned strip. */
  readonly pinnedBotIds: readonly string[];
  readonly activityByBot: Readonly<Record<string, BotActivity>>;
  readonly selection: Selection;
  readonly canControl: boolean;
  readonly onSelect: (selection: Selection) => void;
  /** Creates a placeholder bot in the project; the sidebar tracks the request. */
  readonly onCreateBot: (projectId: string) => void;
  readonly onDeleteBot: (botId: string) => Promise<void>;
  readonly onDeleteProject: (projectId: string) => Promise<void>;
  readonly onTogglePin: (botId: string) => void;
}
