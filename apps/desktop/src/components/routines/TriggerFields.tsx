import type { ReactElement } from "react";
import type { Bot } from "../../protocol/entities";
import type { TriggerDraft, TriggerKind } from "./triggers";

const KINDS: readonly TriggerKind[] = ["cron", "interval", "signal"];
const KIND_LABEL: Readonly<Record<TriggerKind, string>> = {
  cron: "Cron schedule",
  interval: "Interval",
  signal: "Signal",
};

interface TriggerFieldsProps {
  readonly draft: TriggerDraft;
  readonly bots: readonly Bot[];
  readonly onChange: (patch: Partial<TriggerDraft>) => void;
}

export default function TriggerFields({ draft, bots, onChange }: TriggerFieldsProps): ReactElement {
  return (
    <>
      <label className="field">
        <span className="field-label">Trigger</span>
        <select
          value={draft.kind}
          onChange={(event) => {
            const kind = KINDS.find((item) => item === event.target.value);
            if (kind !== undefined) {
              onChange({ kind });
            }
          }}
        >
          {KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {KIND_LABEL[kind]}
            </option>
          ))}
        </select>
      </label>

      {draft.kind === "cron" ? (
        <div className="field-row">
          <label className="field">
            <span className="field-label">Cron expression</span>
            <input
              value={draft.cronExpr}
              onChange={(event) => {
                onChange({ cronExpr: event.target.value });
              }}
            />
          </label>
          <label className="field">
            <span className="field-label">Timezone</span>
            <input
              value={draft.tz}
              onChange={(event) => {
                onChange({ tz: event.target.value });
              }}
            />
          </label>
        </div>
      ) : null}

      {draft.kind === "interval" ? (
        <label className="field">
          <span className="field-label">Every (seconds)</span>
          <input
            inputMode="numeric"
            value={draft.intervalSeconds}
            onChange={(event) => {
              onChange({ intervalSeconds: event.target.value });
            }}
          />
        </label>
      ) : null}

      {draft.kind === "signal" ? (
        <div className="field-row">
          <label className="field">
            <span className="field-label">Signal name</span>
            <input
              value={draft.signalName}
              placeholder="deploy.finished"
              onChange={(event) => {
                onChange({ signalName: event.target.value });
              }}
            />
          </label>
          <label className="field">
            <span className="field-label">From bot</span>
            <select
              value={draft.fromBotId}
              onChange={(event) => {
                onChange({ fromBotId: event.target.value });
              }}
            >
              <option value="">Any bot in the project</option>
              {bots.map((bot) => (
                <option key={bot.id} value={bot.id}>
                  {bot.name}
                </option>
              ))}
            </select>
          </label>
        </div>
      ) : null}
    </>
  );
}
