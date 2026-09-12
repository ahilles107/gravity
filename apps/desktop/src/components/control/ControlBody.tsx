import type { ReactElement, RefObject } from "react";
import type { RegistryFilter } from "./decisions";
import Registry from "./Registry";
import TagsPanel from "./TagsPanel";
import WaitingList from "./WaitingList";
import type { ControlState } from "./useControlState";
import type { DecisionsApi } from "./useDecisions";
import type { TagAdmin } from "./useTagAdmin";

interface ControlBodyProps {
  readonly api: DecisionsApi;
  readonly state: ControlState;
  readonly now: number;
  readonly canControl: boolean;
  readonly tagAdmin: TagAdmin;
  readonly filter: RegistryFilter;
  readonly onQuery: (query: string) => void;
  readonly onTagFilter: (tag?: string) => void;
  readonly onProjectFilter: (projectId?: string) => void;
  readonly onBotFilter: (botId?: string) => void;
  readonly searchRef: RefObject<HTMLInputElement>;
  /** The reading pane, built by the parent; `showBack` on the Settled tab. */
  readonly pane: (showBack: boolean) => ReactElement;
}

/** What sits under the header on each tab. */
export default function ControlBody(props: ControlBodyProps): ReactElement {
  const { api, state, canControl, tagAdmin } = props;

  if (state.tab === "tags") {
    return (
      <div className="control-body">
        <TagsPanel
          tags={api.tags}
          canControl={canControl}
          onAdd={(name) => tagAdmin.upsert(name)}
          onRename={tagAdmin.rename}
          onDescribe={tagAdmin.upsert}
          onDelete={tagAdmin.remove}
        />
      </div>
    );
  }

  if (state.tab === "settled") {
    return (
      <div className="control-body">
        {state.reading === undefined ? (
          <Registry
            rows={api.registry}
            settledCount={api.settled.length}
            tags={api.tags}
            filter={props.filter}
            onQuery={props.onQuery}
            onTagFilter={props.onTagFilter}
            onProjectFilter={props.onProjectFilter}
            onBotFilter={props.onBotFilter}
            searchRef={props.searchRef}
            cursorId={state.cursorId}
            onOpen={state.select}
            projectName={state.projectName}
          />
        ) : (
          props.pane(true)
        )}
      </div>
    );
  }

  return (
    <div className="control-body">
      <WaitingList
        waiting={api.waiting}
        held={api.held}
        settledCount={api.settled.length}
        loaded={api.loaded}
        selectedId={state.selectedId}
        heldOpen={state.heldOpen}
        onToggleHeld={() => state.setHeldOpen(!state.heldOpen)}
        onSelect={state.select}
        now={props.now}
        projectName={state.projectName}
        botName={state.botName}
      />
      {props.pane(false)}
    </div>
  );
}
