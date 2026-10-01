import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { Step, StepDetail } from "../../protocol/chat";
import { errText } from "../../util";
import { groupStartsOpen, groupSummary } from "./chatModel";
import { ImageStrip } from "./ChatImage";
import CodeBlock from "./CodeBlock";
import DiffView from "./DiffView";
import { languageOf } from "./highlight";

interface StepGroupProps {
  readonly client: DaemonApi;
  readonly botId: string;
  readonly steps: readonly Step[];
  readonly turnOpen: boolean;
}

/** A run of tool steps, folded to one summary line once it gets long. */
export default function StepGroup(props: StepGroupProps): ReactElement {
  const { client, botId, steps } = props;
  const [open, setOpen] = useState(() => groupStartsOpen(steps, props.turnOpen));
  const images = steps.flatMap((step) => step.images ?? []);
  return (
    <div className="chat-steps">
      <button
        type="button"
        className="chat-steps-summary"
        aria-expanded={open}
        onClick={() => {
          setOpen((current) => !current);
        }}
      >
        {open ? (
          <ChevronDown className="chat-chevron" size={12} aria-hidden="true" />
        ) : (
          <ChevronRight className="chat-chevron" size={12} aria-hidden="true" />
        )}
        {groupSummary(steps)}
      </button>
      {open ? (
        <ul className="chat-step-list">
          {steps.map((step) => (
            <StepRow key={step.id} client={client} botId={botId} step={step} />
          ))}
        </ul>
      ) : null}
      <ImageStrip client={client} botId={botId} images={images} />
    </div>
  );
}

interface StepRowProps {
  readonly client: DaemonApi;
  readonly botId: string;
  readonly step: Step;
}

type DetailState =
  | { readonly status: "closed" }
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly detail: StepDetail }
  | { readonly status: "failed"; readonly message: string };

function StepRow({ client, botId, step }: StepRowProps): ReactElement {
  const [detail, setDetail] = useState<DetailState>({ status: "closed" });
  const toggle = async (): Promise<void> => {
    if (detail.status !== "closed") {
      setDetail({ status: "closed" });
      return;
    }
    setDetail({ status: "loading" });
    try {
      const reply = await client.request(
        { type: "get_chat_step", bot_id: botId, item_id: step.id },
        "chat_step",
      );
      setDetail({ status: "ready", detail: reply.detail });
    } catch (error) {
      setDetail({ status: "failed", message: errText(error) });
    }
  };
  const lines =
    step.added === undefined && step.removed === undefined
      ? null
      : `+${step.added ?? 0} −${step.removed ?? 0}`;
  return (
    <li className={`chat-step chat-step-${step.status}${step.minor ? " chat-step-minor" : ""}`}>
      <button
        type="button"
        className="chat-step-row"
        aria-expanded={detail.status !== "closed"}
        onClick={() => void toggle()}
      >
        <span className="chat-step-dot" aria-hidden="true" />
        <span className="chat-step-title">{step.title}</span>
        {step.subtitle === undefined ? null : (
          <span className="chat-step-subtitle">{step.subtitle}</span>
        )}
        {lines === null ? null : <span className="chat-step-lines">{lines}</span>}
        {step.status === "running" ? <span className="chat-step-running">running</span> : null}
      </button>
      {detail.status === "loading" ? <div className="chat-step-detail">Loading…</div> : null}
      {detail.status === "failed" ? (
        <div className="chat-step-detail chat-error">{detail.message}</div>
      ) : null}
      {detail.status === "ready" ? (
        <StepDetailView detail={detail.detail} subtitle={step.subtitle} />
      ) : null}
    </li>
  );
}

function StepDetailView({
  detail,
  subtitle,
}: {
  readonly detail: StepDetail;
  readonly subtitle: string | undefined;
}): ReactElement {
  const shown = detail.command ?? detail.diff ?? detail.content;
  return (
    <div className="chat-step-detail">
      {detail.command === undefined ? null : (
        <CodeBlock code={detail.command} language="sh" label="command" />
      )}
      {detail.diff === undefined ? null : <DiffView lines={detail.diff} />}
      {detail.content === undefined ? null : (
        <CodeBlock code={detail.content} language={languageOf(subtitle ?? "")} label={subtitle} />
      )}
      {shown === undefined && detail.input !== undefined ? (
        <CodeBlock code={detail.input} language="json" label="input" />
      ) : null}
      {detail.output === undefined || detail.output === "" ? null : (
        <pre className="chat-step-output">{detail.output}</pre>
      )}
    </div>
  );
}
