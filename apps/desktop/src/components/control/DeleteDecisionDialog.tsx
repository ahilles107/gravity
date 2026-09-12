import type { ReactElement } from "react";
import type { Decision } from "../../protocol/decisions";
import ConfirmDialog from "../overlay/ConfirmDialog";

interface DeleteDecisionDialogProps {
  readonly decision: Decision;
  readonly onCancel: () => void;
  readonly onConfirm: (decisionId: string) => void;
}

/** The one destructive action on a record, and the one place it is confirmed. */
export default function DeleteDecisionDialog(props: DeleteDecisionDialogProps): ReactElement {
  const { decision } = props;
  return (
    <ConfirmDialog
      title="Delete this decision?"
      body={`"${decision.title}" and its thread are removed for good. Bots that were already told keep the message.`}
      confirmLabel="Delete"
      onCancel={props.onCancel}
      onConfirm={() => props.onConfirm(decision.id)}
    />
  );
}
