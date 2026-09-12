import type { ReactElement } from "react";
import { capture } from "../../analytics";
import ControlHeader from "./ControlHeader";
import PublishTray from "./PublishTray";
import type { NotifyCandidate } from "./publishPlan";
import type { ControlActions } from "./useControlActions";
import type { ControlState, ControlTab } from "./useControlState";
import type { DecisionsApi } from "./useDecisions";
import type { NotifySets } from "./useNotifySets";

interface ControlTopProps {
  readonly api: DecisionsApi;
  readonly state: ControlState;
  readonly actions: ControlActions;
  readonly notify: NotifySets;
  readonly candidatesFor: (decisionId: string) => readonly NotifyCandidate[];
  readonly canControl: boolean;
}

/** The header and, when it is open, the publish tray under it. */
export default function ControlTop(props: ControlTopProps): ReactElement {
  const { api, state, actions, notify, canControl } = props;
  const onTab = (tab: ControlTab): void => {
    state.setTab(tab);
    capture("control_center_opened", {});
  };
  const onEdit = (decisionId: string): void => {
    state.setTab("waiting");
    state.select(decisionId);
    state.setTrayOpen(true);
  };
  return (
    <>
      <ControlHeader
        tab={state.tab}
        onTab={onTab}
        counts={{
          waiting: api.waiting.length,
          settled: api.settled.length,
          tags: api.tags.filter((tag) => tag.retired_at === undefined).length,
        }}
        draftCount={api.drafts.length}
        trayOpen={state.trayOpen}
        onToggleTray={() => state.setTrayOpen(!state.trayOpen)}
        canControl={canControl}
      />
      {state.trayOpen ? (
        <PublishTray
          drafts={api.drafts}
          candidatesFor={props.candidatesFor}
          chosenFor={notify.get}
          onToggle={notify.toggle}
          onEdit={onEdit}
          onPublish={actions.publishAll}
          onClose={() => state.setTrayOpen(false)}
          busy={actions.busy}
          canControl={canControl}
        />
      ) : null}
    </>
  );
}
