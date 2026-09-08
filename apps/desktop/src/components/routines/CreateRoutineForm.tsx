import { useState } from "react";
import type { ReactElement } from "react";
import type { Bot, OverlapPolicy, RoutineTrigger } from "../../protocol/entities";
import TriggerFields from "./TriggerFields";
import type { RoutineLimits } from "./useRoutines";
import { buildTrigger, EMPTY_TRIGGER_DRAFT } from "./triggers";
import type { TriggerDraft } from "./triggers";

const OVERLAP_POLICIES: readonly OverlapPolicy[] = ["skip", "queue_one", "queue_all", "replace"];
const MAX_DURATION_SECONDS = 6 * 60 * 60;
const MAX_ATTEMPTS = 5;

interface ParsedLimit {
  readonly value?: number;
  readonly error?: string;
}

/** Blank means "leave it to the daemon default", not zero. */
function boundedPositiveInt(raw: string, max: number, label: string): ParsedLimit {
  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    return {};
  }
  if (!/^\d+$/.test(trimmed)) {
    return { error: `${label} must be a whole number.` };
  }
  const value = Number(trimmed);
  if (!Number.isSafeInteger(value) || value < 1 || value > max) {
    return { error: `${label} must be between 1 and ${max}.` };
  }
  return { value };
}

interface CreateRoutineFormProps {
  readonly bots: readonly Bot[];
  readonly onSubmit: (
    name: string,
    trigger: RoutineTrigger,
    prompt: string,
    overlapPolicy: OverlapPolicy,
    limits: RoutineLimits,
  ) => Promise<boolean>;
  readonly onClose: () => void;
}

export default function CreateRoutineForm({
  bots,
  onSubmit,
  onClose,
}: CreateRoutineFormProps): ReactElement {
  const [name, setName] = useState("");
  const [prompt, setPrompt] = useState("");
  const [overlapPolicy, setOverlapPolicy] = useState<OverlapPolicy>("skip");
  const [maxDuration, setMaxDuration] = useState("");
  const [maxAttempts, setMaxAttempts] = useState("");
  const [submitting, setSubmitting] = useState(false);
  // The empty source means any bot in the project, which is the useful default.
  const [draft, setDraft] = useState<TriggerDraft>(EMPTY_TRIGGER_DRAFT);

  const duration = boundedPositiveInt(maxDuration, MAX_DURATION_SECONDS, "Duration");
  const attempts = boundedPositiveInt(maxAttempts, MAX_ATTEMPTS, "Attempts");

  const submit = async (): Promise<void> => {
    const trigger = buildTrigger(draft);
    if (
      name.trim().length === 0 ||
      prompt.trim().length === 0 ||
      trigger === null ||
      duration.error !== undefined ||
      attempts.error !== undefined
    ) {
      return;
    }
    setSubmitting(true);
    const created = await onSubmit(name.trim(), trigger, prompt, overlapPolicy, {
      maxDurationSeconds: duration.value,
      maxAttempts: attempts.value,
    });
    setSubmitting(false);
    if (created) {
      onClose();
    }
  };

  return (
    <form
      className="routine-form"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <label className="field">
        <span className="field-label">Name</span>
        <input
          autoFocus
          value={name}
          placeholder="weekly-report"
          onChange={(event) => {
            setName(event.target.value);
          }}
        />
      </label>

      <TriggerFields
        draft={draft}
        bots={bots}
        onChange={(patch) => {
          setDraft((prev) => ({ ...prev, ...patch }));
        }}
      />

      <label className="field">
        <span className="field-label">Prompt</span>
        <textarea
          rows={4}
          placeholder="/weekly-report"
          value={prompt}
          onChange={(event) => {
            setPrompt(event.target.value);
          }}
        />
      </label>

      <label className="field">
        <span className="field-label">Overlap policy</span>
        <select
          value={overlapPolicy}
          onChange={(event) => {
            const match = OVERLAP_POLICIES.find((policy) => policy === event.target.value);
            if (match !== undefined) {
              setOverlapPolicy(match);
            }
          }}
        >
          {OVERLAP_POLICIES.map((policy) => (
            <option key={policy} value={policy}>
              {policy}
            </option>
          ))}
        </select>
      </label>

      <div className="field-row">
        <label className="field">
          <span className="field-label">Give up after (seconds)</span>
          <input
            type="number"
            min={1}
            max={MAX_DURATION_SECONDS}
            step={1}
            inputMode="numeric"
            placeholder="default"
            value={maxDuration}
            onChange={(event) => {
              setMaxDuration(event.target.value);
            }}
          />
          {duration.error === undefined ? null : (
            <span className="field-error">{duration.error}</span>
          )}
        </label>
        <label className="field">
          <span className="field-label">Attempts per run</span>
          <input
            type="number"
            min={1}
            max={MAX_ATTEMPTS}
            step={1}
            inputMode="numeric"
            placeholder="1"
            value={maxAttempts}
            onChange={(event) => {
              setMaxAttempts(event.target.value);
            }}
          />
          {attempts.error === undefined ? null : (
            <span className="field-error">{attempts.error}</span>
          )}
        </label>
      </div>

      <div className="panel-actions">
        <button type="submit" className="btn btn-primary" disabled={submitting}>
          Create routine
        </button>
        <button type="button" className="btn" onClick={onClose}>
          Cancel
        </button>
      </div>
    </form>
  );
}
