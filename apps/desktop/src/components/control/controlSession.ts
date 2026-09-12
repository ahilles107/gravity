import type { ControlTab } from "./useControlState";

/**
 * The Control center's view state, kept outside React for the life of the window.
 *
 * The view unmounts whenever a bot is opened, so without this a glance at a bot
 * would throw away the tab, the record being read and the search behind it.
 */
export interface ControlView {
  readonly tab: ControlTab;
  readonly waitingId: string | undefined;
  readonly settledId: string | undefined;
  readonly heldOpen: boolean;
  readonly query: string;
  readonly tagFilter: string | undefined;
  readonly projectFilter: string | undefined;
  readonly botFilter: string | undefined;
}

const EMPTY: ControlView = {
  tab: "waiting",
  waitingId: undefined,
  settledId: undefined,
  heldOpen: false,
  query: "",
  tagFilter: undefined,
  projectFilter: undefined,
  botFilter: undefined,
};

let view: ControlView = EMPTY;
const offsets = new Map<string, number>();

export function controlView(): ControlView {
  return view;
}

/** Each hook writes only the slice it owns. */
export function rememberControlView(next: Partial<ControlView>): void {
  view = { ...view, ...next };
}

/** Where the reader was left on a record, or its top if it was never scrolled. */
export function readerOffset(decisionId: string): number {
  return offsets.get(decisionId) ?? 0;
}

export function rememberReaderOffset(decisionId: string, top: number): void {
  offsets.set(decisionId, top);
}

/** Tests share the module, so each one starts from a clean view. */
export function forgetControlSession(): void {
  view = EMPTY;
  offsets.clear();
}
