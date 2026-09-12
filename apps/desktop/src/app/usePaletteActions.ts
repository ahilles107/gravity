import { useMemo } from "react";
import type { PaletteAction } from "../components/CommandPalette";
import type { SettingsCategory } from "../components/settings/categories";
import type { Bot } from "../protocol/entities";
import type { Selection } from "./selection";

interface PaletteDeps {
  readonly bots: readonly Bot[];
  readonly select: (next: Selection) => void;
  readonly openSettings: (category?: SettingsCategory) => void;
}

/** Cmd+K entries: navigation for every bot and view. Bots are always-on, so
    there is nothing to start or stop here. */
export function usePaletteActions(deps: PaletteDeps): readonly PaletteAction[] {
  const { bots, select, openSettings } = deps;

  return useMemo((): readonly PaletteAction[] => {
    return [
      ...bots.map((bot) => ({
        id: `open-bot-${bot.id}`,
        label: bot.name,
        hint: "open bot",
        run: (): void => {
          select({ kind: "bot", botId: bot.id });
        },
      })),
      {
        id: "open-control-center",
        label: "Control center",
        hint: "open view",
        run: (): void => {
          select({ kind: "control" });
        },
      },
      {
        id: "open-settings",
        label: "Settings",
        hint: "open view",
        run: (): void => {
          openSettings();
        },
      },
      {
        id: "open-diagnostics",
        label: "Diagnostics",
        hint: "open view",
        run: (): void => {
          openSettings("diagnostics");
        },
      },
    ];
  }, [bots, select, openSettings]);
}
