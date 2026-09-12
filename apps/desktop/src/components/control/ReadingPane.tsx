import type { ReactElement, RefObject } from "react";
import type { Decision } from "../../protocol/decisions";
import Composer from "./Composer";
import DecisionReader from "./DecisionReader";
import type { NotifyCandidate } from "./publishPlan";
import type { ControlActions } from "./useControlActions";
import type { Composer as ComposerState } from "./useComposer";
import type { ControlState } from "./useControlState";
import type { DecisionsApi } from "./useDecisions";
import type { NotifySets } from "./useNotifySets";
import { useReaderScroll } from "./useReaderScroll";

interface ReadingPaneProps {
  readonly decision: Decision | undefined;
  readonly showBack: boolean;
  readonly now: number;
  readonly canControl: boolean;
  readonly api: DecisionsApi;
  readonly state: ControlState;
  readonly composer: ComposerState;
  readonly actions: ControlActions;
  readonly notify: NotifySets;
  readonly candidatesFor: (decisionId: string) => readonly NotifyCandidate[];
  readonly botAvatar: (botId: string) => { avatar: string; name: string } | undefined;
  readonly textareaRef: RefObject<HTMLTextAreaElement>;
  readonly onDelete: (decision: Decision) => void;
}

/** The article plus, for an open decision, the composer docked under it. */
export default function ReadingPane(props: ReadingPaneProps): ReactElement {
  const { decision, api, state, composer, actions, notify, canControl } = props;
  const scrollRef = useReaderScroll(decision?.id);
  if (decision === undefined) {
    return (
      <section className="cc-reader">
        <div className="cc-reader-none">Select a decision</div>
      </section>
    );
  }
  return (
    <section className="cc-reader">
      <div className="cc-reader-scroll" ref={scrollRef}>
        <DecisionReader
          key={decision.id}
          decision={decision}
          now={props.now}
          canControl={canControl}
          showBack={props.showBack}
          onBack={state.back}
          showQuestion={state.showQuestion}
          onToggleQuestion={() => state.setShowQuestion(!state.showQuestion)}
          picked={composer.pick}
          onPick={composer.togglePick}
          projectName={state.projectName}
          botName={state.botName}
          botAvatar={props.botAvatar}
          candidates={props.candidatesFor(decision.id)}
          chosen={notify.get(decision.id)}
          onToggleNotify={(botId) => notify.toggle(decision.id, botId)}
          onEditDraft={actions.editDraft}
          onOpenTray={() => state.setTrayOpen(true)}
          onResume={() => void api.resume(decision.id)}
          onConfirm={() => void api.confirm(decision.id)}
          onDelete={() => props.onDelete(decision)}
        />
      </div>
      {decision.state === "open" ? (
        <Composer
          decision={decision}
          composer={composer}
          canControl={canControl}
          busy={actions.busy}
          textareaRef={props.textareaRef}
          onSave={actions.saveRuling}
          onAsk={actions.askInThread}
          onHold={actions.hold}
        />
      ) : null}
    </section>
  );
}
