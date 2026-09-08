import { useCallback, useState } from "react";
import type { MutableRefObject } from "react";
import type { Bot, Project } from "../protocol/entities";
import { saveLastUsedBotId } from "../settings";
import type { Selection } from "./selection";
import { useInitialBot } from "./useInitialBot";
import { useLatestRef } from "./useLatestRef";
import type { UnreadApi } from "./useUnread";

export interface SelectionApi {
  /** What the main pane is currently showing. */
  readonly selection: Selection;
  /** The rendered selection, for callbacks that fire outside React. */
  readonly selectionRef: MutableRefObject<Selection>;
  /** Opens a view and clears whatever badge it was carrying. */
  readonly select: (next: Selection) => void;
  /** Opens a bot by id. */
  readonly selectBot: (botId: string) => void;
}

/**
 * The current selection and the ways it changes: an explicit pick, a bot opened
 * by id, and the one-off startup pick made by `useInitialBot`. Selecting always
 * clears the target's unread badge, so the two stay in step here rather than at
 * each call site.
 */
export function useSelection(
  projects: readonly Project[],
  bots: readonly Bot[],
  unread: UnreadApi,
): SelectionApi {
  const [selection, setSelection] = useState<Selection>({ kind: "none" });
  const selectionRef = useLatestRef(selection);

  const select = useCallback(
    (next: Selection): void => {
      setSelection(next);
      unread.clearFor(next);
      if (next.kind === "bot") {
        saveLastUsedBotId(next.botId);
      }
    },
    [unread],
  );

  const selectBot = useCallback(
    (botId: string): void => {
      select({ kind: "bot", botId });
    },
    [select],
  );

  useInitialBot(projects, bots, selection, select);

  return { selection, selectionRef, select, selectBot };
}
