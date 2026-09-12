import { useCallback, useEffect, useMemo, useState } from "react";
import type { Decision } from "../../protocol/decisions";
import type { Bot, Project } from "../../protocol/entities";
import { controlView, rememberControlView } from "./controlSession";
import type { DecisionsApi } from "./useDecisions";

export type ControlTab = "waiting" | "settled" | "tags";

export interface ControlState {
  readonly tab: ControlTab;
  readonly setTab: (tab: ControlTab) => void;
  /** What the reading pane shows: the Waiting selection, or a registry row opened in place. */
  readonly reading: Decision | undefined;
  /** The highlighted row on the current tab. */
  readonly selectedId: string | undefined;
  readonly select: (decisionId: string) => void;
  /** The keyboard cursor: the Waiting selection, or the highlighted ledger row. */
  readonly cursorId: string | undefined;
  /** Move the cursor without opening — on the ledger only; elsewhere it selects. */
  readonly moveCursor: (decisionId: string) => void;
  /** Settled tab: leave the reading pane for the ledger. */
  readonly back: () => void;
  /** Switch to Settled and read this ruling. */
  readonly trayOpen: boolean;
  readonly setTrayOpen: (open: boolean) => void;
  readonly heldOpen: boolean;
  readonly setHeldOpen: (open: boolean) => void;
  readonly showQuestion: boolean;
  readonly setShowQuestion: (open: boolean) => void;
  readonly projectName: (projectId: string) => string;
  readonly botName: (botId: string) => string | undefined;
  readonly leadFor: (projectId: string) => string | undefined;
}

interface Stored {
  readonly tab: ControlTab;
  readonly waitingId: string | undefined;
  readonly settledId: string | undefined;
  /** The `decisionId` prop this state was derived from, so it is applied once. */
  readonly resolvedFor: string | undefined;
}

/**
 * The view's own state: which tab, which row, and the small toggles.
 *
 * The Waiting selection is validated against the store on every render, so a
 * row that was published or deleted under the cursor falls back to the first
 * waiting item without an effect.
 */
export function useControlState(
  api: DecisionsApi,
  projects: readonly Project[],
  bots: readonly Bot[],
  initialDecisionId?: string,
): ControlState {
  const [stored, setStored] = useState<Stored>(() => {
    const saved = controlView();
    return {
      tab: saved.tab,
      waitingId: initialDecisionId ?? saved.waitingId,
      settledId: saved.settledId,
      resolvedFor: undefined,
    };
  });
  const [trayOpen, setTrayOpen] = useState(false);
  const [heldOpen, setHeldOpen] = useState(() => controlView().heldOpen);
  const [showQuestion, setShowQuestion] = useState(false);

  const state = applyIncoming(stored, setStored, setHeldOpen, api, initialDecisionId);

  const waitingSelected = useMemo(() => pickWaiting(api, state.waitingId), [api, state.waitingId]);
  const settledSelected = useMemo(
    () => (state.settledId === undefined ? undefined : api.byId.get(state.settledId)),
    [api.byId, state.settledId],
  );
  const reading = readingFor(state.tab, waitingSelected, settledSelected);
  const readingId = reading?.id;

  // Opening a bot unmounts the view, so what it was showing is kept outside it.
  // The Waiting row is stored as resolved, so a record that was only the top of
  // the list is still the one being read when the view comes back.
  const waitingId = waitingSelected?.id ?? state.waitingId;
  useEffect(() => {
    rememberControlView({
      tab: state.tab,
      waitingId,
      settledId: state.settledId,
      heldOpen,
    });
  }, [state.tab, waitingId, state.settledId, heldOpen]);

  const setTab = useCallback((tab: ControlTab): void => {
    setStored((prev) => withTab(prev, tab));
    setTrayOpen(false);
    setShowQuestion(false);
  }, []);

  const select = useCallback((decisionId: string): void => {
    setStored((prev) => withSelection(prev, decisionId));
    setShowQuestion(false);
  }, []);

  const ledger = useLedgerCursor({
    onLedger: ledgerShowing(state.tab, settledSelected),
    openId: state.settledId,
    selectedId: readingId,
    select,
  });

  const { leaveReader } = ledger;
  const back = useCallback((): void => {
    leaveReader();
    setStored((prev) => ({ ...prev, settledId: undefined }));
    setShowQuestion(false);
  }, [leaveReader]);

  const names = useNames(projects, bots);

  return {
    tab: state.tab,
    setTab,
    reading,
    selectedId: readingId,
    select,
    cursorId: ledger.cursorId,
    moveCursor: ledger.moveCursor,
    back,
    trayOpen,
    setTrayOpen,
    heldOpen,
    setHeldOpen,
    showQuestion,
    setShowQuestion,
    ...names,
  };
}

/** The ledger is on screen when the Settled tab has no ruling open over it. */
function ledgerShowing(tab: ControlTab, settledSelected: Decision | undefined): boolean {
  return tab === "settled" && settledSelected === undefined;
}

interface LedgerCursorOptions {
  /** True while the ledger itself is on screen, with no ruling open over it. */
  readonly onLedger: boolean;
  /** The ruling the reader is showing, if any. */
  readonly openId: string | undefined;
  /** Where the cursor sits everywhere else: the current selection. */
  readonly selectedId: string | undefined;
  readonly select: (decisionId: string) => void;
}

interface LedgerCursor {
  readonly cursorId: string | undefined;
  readonly moveCursor: (decisionId: string) => void;
  /** Closing the reader leaves the cursor on the ruling that was open. */
  readonly leaveReader: () => void;
}

/**
 * The ledger's own cursor.
 *
 * A ruling is highlighted before it is opened, so arrowing down the registry
 * reads as moving through a list rather than opening every record in turn.
 * Anywhere else the cursor is the selection, and moving it selects.
 */
function useLedgerCursor(options: LedgerCursorOptions): LedgerCursor {
  const { onLedger, openId, selectedId, select } = options;
  const [cursor, setCursor] = useState<string | undefined>(undefined);
  const moveCursor = useCallback(
    (decisionId: string): void => {
      if (onLedger) {
        setCursor(decisionId);
        return;
      }
      select(decisionId);
    },
    [onLedger, select],
  );
  const leaveReader = useCallback((): void => {
    setCursor(openId ?? cursor);
  }, [cursor, openId]);
  return { cursorId: onLedger ? cursor : selectedId, moveCursor, leaveReader };
}

interface Names {
  readonly projectName: (projectId: string) => string;
  readonly botName: (botId: string) => string | undefined;
  readonly leadFor: (projectId: string) => string | undefined;
}

function useNames(projects: readonly Project[], bots: readonly Bot[]): Names {
  const projectName = useCallback(
    (projectId: string) => projects.find((item) => item.id === projectId)?.name ?? "—",
    [projects],
  );
  const botName = useCallback(
    (botId: string) => bots.find((bot) => bot.id === botId)?.name,
    [bots],
  );
  const leadFor = useCallback(
    (projectId: string) => projects.find((item) => item.id === projectId)?.lead_bot_id ?? undefined,
    [projects],
  );
  return { projectName, botName, leadFor };
}

/**
 * A notification hands over an id before the lists have loaded; once they have,
 * the record says which tab and row it belongs on.
 */
function applyIncoming(
  stored: Stored,
  setStored: (next: Stored) => void,
  setHeldOpen: (open: boolean) => void,
  api: DecisionsApi,
  initialDecisionId: string | undefined,
): Stored {
  const incoming = resolveIncoming(stored, api, initialDecisionId);
  if (incoming === undefined) {
    return stored;
  }
  setStored(incoming.next);
  if (incoming.expandHeld) {
    setHeldOpen(true);
  }
  return incoming.next;
}

/** The tab and row an incoming `decisionId` lands on, or nothing once it has been applied. */
function resolveIncoming(
  stored: Stored,
  api: DecisionsApi,
  initialDecisionId: string | undefined,
): { next: Stored; expandHeld: boolean } | undefined {
  if (!api.loaded || initialDecisionId === undefined || stored.resolvedFor === initialDecisionId) {
    return undefined;
  }
  const target = api.byId.get(initialDecisionId);
  const onRegistry = target?.state === "settled" || target?.state === "withdrawn";
  return {
    next: {
      tab: onRegistry ? "settled" : "waiting",
      waitingId: onRegistry ? stored.waitingId : initialDecisionId,
      settledId: onRegistry ? initialDecisionId : undefined,
      resolvedFor: initialDecisionId,
    },
    expandHeld: target?.state === "held",
  };
}

/** The stored Waiting selection while it is still waiting or held, else the first row. */
function pickWaiting(api: DecisionsApi, waitingId: string | undefined): Decision | undefined {
  const candidate = waitingId === undefined ? undefined : api.byId.get(waitingId);
  if (candidate !== undefined && candidate.state !== "settled" && candidate.state !== "withdrawn") {
    return candidate;
  }
  return api.waiting[0];
}

/** Switching to Settled always lands on the ledger, never on a stale reading pane. */
function withTab(prev: Stored, tab: ControlTab): Stored {
  return {
    ...prev,
    tab,
    settledId: tab === "settled" ? undefined : prev.settledId,
  };
}

function withSelection(prev: Stored, decisionId: string): Stored {
  if (prev.tab === "settled") {
    return { ...prev, settledId: decisionId };
  }
  return { ...prev, waitingId: decisionId };
}

function readingFor(
  tab: ControlTab,
  waiting: Decision | undefined,
  settled: Decision | undefined,
): Decision | undefined {
  if (tab === "waiting") {
    return waiting;
  }
  return tab === "settled" ? settled : undefined;
}
