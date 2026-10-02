import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { Decision } from "../../protocol/decisions";
import { errText } from "../../util";

interface DecisionCardProps {
  readonly client: DaemonApi;
  /** Absent until the raise's result is in the transcript. */
  readonly decisionId: string | undefined;
  readonly title: string;
  readonly connected: boolean;
  readonly onOpenDecision?: (decisionId: string) => void;
}

const STATE_LABEL: Readonly<Record<Decision["state"], string>> = {
  open: "Waiting on you",
  answered: "Answered, not yet published",
  held: "On hold",
  settled: "Settled",
  withdrawn: "Withdrawn",
};

/**
 * A decision the bot raised, answerable where it was asked. Answering here
 * publishes in one step, exactly as "Answer and publish" in the Control
 * Center does; drafts, holds and the discussion stay in the Control Center.
 */
export default function DecisionCard(props: DecisionCardProps): ReactElement {
  const { client, decisionId, connected } = props;
  const [decision, setDecision] = useState<Decision | null>(null);
  const [other, setOther] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<void> => {
    if (decisionId === undefined) {
      return;
    }
    try {
      const reply = await client.request(
        { type: "get_decision", decision_id: decisionId },
        "decision",
      );
      setDecision(reply.decision);
    } catch (failure) {
      setError(errText(failure));
    }
  }, [client, decisionId]);

  useLoadOnConnect(connected, load);

  useEffect(
    () =>
      client.on("decision_update", (push) => {
        if (push.decision.id === decisionId) {
          setDecision(push.decision);
        }
      }),
    [client, decisionId],
  );

  const publish = async (option: string | undefined, text: string): Promise<void> => {
    if (decisionId === undefined || busy) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await client.request(
        {
          type: "publish_decisions",
          items: [
            {
              decision_id: decisionId,
              ruling_text: text,
              ...(option === undefined ? {} : { ruling_option: option }),
            },
          ],
        },
        "publish_result",
      );
      await load();
    } catch (failure) {
      setError(errText(failure));
    } finally {
      setBusy(false);
    }
  };

  const canAnswer = connected && client.hasGrant("approve") && decision?.state === "open";
  return (
    <div className="chat-card chat-decision">
      <div className="chat-card-label">
        Asked you to decide{decision === null ? "" : ` · ${STATE_LABEL[decision.state]}`}
      </div>
      <div className="chat-decision-title">{props.title}</div>
      {decision?.ruling === undefined ? null : (
        <div className="chat-decision-ruling">You answered: {decision.ruling.text}</div>
      )}
      {canAnswer && decision !== null ? (
        <div className="chat-decision-options">
          {decision.options.map((option) => (
            <button
              key={option.key}
              type="button"
              className={`btn btn-small${option.key === decision.recommendation ? " btn-primary" : ""}`}
              title={option.description}
              disabled={busy}
              onClick={() => void publish(option.key, option.label)}
            >
              {option.label}
            </button>
          ))}
          <input
            className="chat-decision-other"
            aria-label="Another answer"
            placeholder="Or answer in your own words"
            value={other}
            disabled={busy}
            onChange={(event) => {
              setOther(event.target.value);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && other.trim() !== "") {
                void publish(undefined, other.trim());
              }
            }}
          />
        </div>
      ) : null}
      {error === null ? null : <div className="chat-note chat-error">{error}</div>}
      {decisionId === undefined || props.onOpenDecision === undefined ? null : (
        <button
          type="button"
          className="permission-toggle"
          onClick={() => {
            props.onOpenDecision?.(decisionId);
          }}
        >
          Open in the Control Center
        </button>
      )}
    </div>
  );
}
