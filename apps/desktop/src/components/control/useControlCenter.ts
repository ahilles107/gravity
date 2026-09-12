import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Decision } from "../../protocol/decisions";
import type { Bot, NotifyLevel, Project } from "../../protocol/entities";
import { controlView, rememberControlView } from "./controlSession";
import type { RegistryFilter } from "./decisions";
import type { BotLookups } from "./useBotLookups";
import { useBotLookups } from "./useBotLookups";
import { useComposer } from "./useComposer";
import type { Composer } from "./useComposer";
import { useControlActions } from "./useControlActions";
import type { ControlActions } from "./useControlActions";
import { navListFor, useControlKeys } from "./useControlKeys";
import { useControlState } from "./useControlState";
import type { ControlState } from "./useControlState";
import { useDecisionDetail } from "./useDecisionDetail";
import { useDecisions } from "./useDecisions";
import type { DecisionsApi } from "./useDecisions";
import { useNotifySets } from "./useNotifySets";
import type { NotifySets } from "./useNotifySets";
import { useNow } from "./useNow";
import { useTagAdmin } from "./useTagAdmin";
import type { TagAdmin } from "./useTagAdmin";

export interface ControlCenterOptions {
  readonly client: DaemonApi;
  readonly projects: readonly Project[];
  readonly bots: readonly Bot[];
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly decisionId?: string;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

export interface ControlCenter extends BotLookups, RegistryFilterState {
  readonly api: DecisionsApi;
  readonly state: ControlState;
  readonly tagAdmin: TagAdmin;
  readonly now: number;
  readonly composer: Composer;
  readonly notify: NotifySets;
  readonly actions: ControlActions;
  readonly textareaRef: RefObject<HTMLTextAreaElement>;
  readonly searchRef: RefObject<HTMLInputElement>;
  readonly deleting: Decision | undefined;
  readonly setDeleting: (decision: Decision | undefined) => void;
}

interface ControlData {
  readonly api: DecisionsApi;
  readonly state: ControlState;
  readonly tagAdmin: TagAdmin;
  readonly now: number;
}

/** The registry, the view state over it, and the clock. */
function useControlData(options: ControlCenterOptions): ControlData {
  const { client, onToast } = options;
  const api = useDecisions(client, options.connected, onToast);
  const state = useControlState(api, options.projects, options.bots, options.decisionId);
  // A rename shows on every decision's tags, so both lists come back.
  const { reload, reloadTags } = api;
  const reloadAll = useCallback(async (): Promise<void> => {
    await Promise.all([reload(), reloadTags()]);
  }, [reload, reloadTags]);
  const tagAdmin = useTagAdmin(client, reloadAll, onToast);
  const now = useNow();
  useDecisionDetail(client, state.reading, api.replace);
  return { api, state, tagAdmin, now };
}

interface ControlEditing extends BotLookups {
  readonly composer: Composer;
  readonly notify: NotifySets;
  readonly actions: ControlActions;
  readonly textareaRef: RefObject<HTMLTextAreaElement>;
  readonly searchRef: RefObject<HTMLInputElement>;
}

interface RegistryFilterState {
  /** One object, stable while the filter is, so memos over it hold across clock ticks. */
  readonly filter: RegistryFilter;
  readonly setQuery: (query: string) => void;
  readonly setTagFilter: (tag?: string) => void;
  readonly setProjectFilter: (projectId?: string) => void;
  readonly setBotFilter: (botId?: string) => void;
}

/** What the ledger is narrowed to, remembered across visits. */
function useRegistryFilter(): RegistryFilterState {
  const [query, setQuery] = useState(() => controlView().query);
  const [tagFilter, setTagFilter] = useState<string | undefined>(() => controlView().tagFilter);
  const [projectFilter, setProject] = useState<string | undefined>(
    () => controlView().projectFilter,
  );
  const [botFilter, setBotFilter] = useState<string | undefined>(() => controlView().botFilter);
  useEffect(() => {
    rememberControlView({ query, tagFilter, projectFilter, botFilter });
  }, [query, tagFilter, projectFilter, botFilter]);
  // A bot belongs to one project, so a bot chosen under the old project would
  // leave the ledger empty with no chip on screen explaining why.
  const setProjectFilter = useCallback((projectId?: string): void => {
    setProject(projectId);
    setBotFilter(undefined);
  }, []);
  // Stable while the filter is, so the ledger and the nav list are not
  // refiltered on every clock tick.
  const filter = useMemo(
    () => ({ query, tagFilter, projectFilter, botFilter }),
    [query, tagFilter, projectFilter, botFilter],
  );
  return { filter, setQuery, setTagFilter, setProjectFilter, setBotFilter };
}

/** The composer, the notify sets, the verbs and the keys, bound to what is being read. */
function useControlEditing(
  data: ControlData,
  options: ControlCenterOptions,
  filter: RegistryFilter,
): ControlEditing {
  const { api, state } = data;
  const { canControl } = options;
  const { reading } = state;
  const composer = useComposer(reading?.id);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const lookups = useBotLookups(options.bots, api, state.leadFor);
  const notify = useNotifySets(lookups.candidatesFor);
  const actions = useControlActions({ api, state, composer, notify, reading, canControl });
  const navList = useMemo(
    () => navListFor(state.tab, api, state.heldOpen, filter),
    [api, filter, state.heldOpen, state.tab],
  );
  useControlKeys({
    tab: state.tab,
    setTab: state.setTab,
    navList,
    cursorId: state.cursorId,
    moveCursor: state.moveCursor,
    select: state.select,
    reading,
    readingOpen: reading !== undefined,
    back: state.back,
    trayOpen: state.trayOpen,
    setTrayOpen: state.setTrayOpen,
    hasDrafts: api.drafts.length > 0,
    holdOpen: composer.holdOpen,
    setHoldOpen: composer.setHoldOpen,
    togglePick: composer.togglePick,
    saveRuling: actions.saveRuling,
    askInThread: actions.askInThread,
    textareaRef,
    searchRef,
  });
  return { composer, notify, actions, textareaRef, searchRef, ...lookups };
}

/** Everything the Control center view needs, wired together. */
export function useControlCenter(options: ControlCenterOptions): ControlCenter {
  const data = useControlData(options);
  const filter = useRegistryFilter();
  const editing = useControlEditing(data, options, filter.filter);
  const [deleting, setDeleting] = useState<Decision | undefined>(undefined);
  return { ...data, ...editing, ...filter, deleting, setDeleting };
}
