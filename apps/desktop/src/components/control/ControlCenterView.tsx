import type { ReactElement } from "react";
import ControlBody from "./ControlBody";
import ControlTop from "./ControlTop";
import DeleteDecisionDialog from "./DeleteDecisionDialog";
import ReadingPane from "./ReadingPane";
import { useControlCenter } from "./useControlCenter";
import type { ControlCenterOptions } from "./useControlCenter";

/** One inbox for every question a bot has asked, across every project. */
export default function ControlCenterView(props: ControlCenterOptions): ReactElement {
  const cc = useControlCenter(props);
  const { api, state, actions, notify, canControl } = { ...cc, canControl: props.canControl };

  const pane = (showBack: boolean): ReactElement => (
    <ReadingPane
      decision={state.reading}
      showBack={showBack}
      now={cc.now}
      canControl={canControl}
      api={api}
      state={state}
      composer={cc.composer}
      actions={actions}
      notify={notify}
      candidatesFor={cc.candidatesFor}
      botAvatar={cc.botAvatar}
      textareaRef={cc.textareaRef}
      onDelete={cc.setDeleting}
    />
  );

  return (
    <div className="control-center">
      <ControlTop
        api={api}
        state={state}
        actions={actions}
        notify={notify}
        candidatesFor={cc.candidatesFor}
        canControl={canControl}
      />
      <ControlBody
        api={api}
        state={state}
        now={cc.now}
        canControl={canControl}
        tagAdmin={cc.tagAdmin}
        filter={cc.filter}
        onQuery={cc.setQuery}
        onTagFilter={cc.setTagFilter}
        onProjectFilter={cc.setProjectFilter}
        onBotFilter={cc.setBotFilter}
        searchRef={cc.searchRef}
        pane={pane}
      />
      {cc.deleting === undefined ? null : (
        <DeleteDecisionDialog
          decision={cc.deleting}
          onCancel={() => cc.setDeleting(undefined)}
          onConfirm={(id) => {
            cc.setDeleting(undefined);
            void api.remove(id);
          }}
        />
      )}
    </div>
  );
}
