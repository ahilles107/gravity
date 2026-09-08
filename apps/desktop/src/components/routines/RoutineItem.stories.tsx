import type { Story } from "@ladle/react";
import { bot, routine } from "../../test/fixtures";
import RoutineItem from "./RoutineItem";

const noop = (): void => {};
const BOTS = [bot()] as const;

export const Cron: Story = () => (
  <ul style={{ listStyle: "none", margin: 0, padding: 0, maxWidth: 560 }}>
    <RoutineItem
      routine={routine()}
      bots={BOTS}
      connected
      canControl
      expanded={false}
      onToggleEnabled={noop}
      onRunNow={noop}
      onToggleHistory={noop}
    />
  </ul>
);

export const IntervalDisabled: Story = () => (
  <ul style={{ listStyle: "none", margin: 0, padding: 0, maxWidth: 560 }}>
    <RoutineItem
      routine={routine({
        name: "poll-ci",
        trigger: { kind: "interval", seconds: 300 },
        prompt: "/check-ci",
        enabled: false,
        next_run_at: null,
      })}
      bots={BOTS}
      connected
      canControl
      expanded={false}
      onToggleEnabled={noop}
      onRunNow={noop}
      onToggleHistory={noop}
    />
  </ul>
);

export const SignalTrigger: Story = () => (
  <ul style={{ listStyle: "none", margin: 0, padding: 0, maxWidth: 560 }}>
    <RoutineItem
      routine={routine({
        name: "summarise-done",
        trigger: { kind: "signal", name: "bot_done", from_bot_id: "b1" },
        prompt: "/summarise the finished task",
      })}
      bots={BOTS}
      connected
      canControl
      expanded
      onToggleEnabled={noop}
      onRunNow={noop}
      onToggleHistory={noop}
    >
      <div className="muted">History goes here when expanded.</div>
    </RoutineItem>
  </ul>
);

const LONG_PROMPT = [
  "Monthly written audit for the month just ended. Read workspace/FACTS.md first — it is authoritative and supersedes any number or instruction in this prompt if newer.",
  "BASIS — non-negotiable: work POSTED-ONLY and state the basis on every figure. Never mix a breakdowns figure with a composition figure from the same response.",
  "SPAN — before quoting ANY month-on-month delta, establish what span the series is actually reporting. A delta over the wrong span reads as authoritative and is wrong.",
  "Deliver every month, whether or not anything is wrong: spend by category against budget, liquid position and net worth month on month, progress against the deposit goal.",
  "Then update the runway note where the month changed something and say explicitly which figures you re-derived this run and which you carried forward.",
].join("\n\n");

export const LongPrompt: Story = () => (
  <ul style={{ listStyle: "none", margin: 0, padding: 0, maxWidth: 560 }}>
    <RoutineItem
      routine={routine({ name: "monthly-finance-audit", prompt: LONG_PROMPT })}
      bots={BOTS}
      connected
      canControl
      expanded={false}
      onToggleEnabled={noop}
      onRunNow={noop}
      onToggleHistory={noop}
    />
  </ul>
);

export const ReadOnlyDisconnected: Story = () => (
  <ul style={{ listStyle: "none", margin: 0, padding: 0, maxWidth: 560 }}>
    <RoutineItem
      routine={routine()}
      bots={BOTS}
      connected={false}
      canControl={false}
      expanded={false}
      onToggleEnabled={noop}
      onRunNow={noop}
      onToggleHistory={noop}
    />
  </ul>
);
