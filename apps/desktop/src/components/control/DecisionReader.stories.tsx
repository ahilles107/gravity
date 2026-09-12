import type { Story } from "@ladle/react";
import type { ReactElement, ReactNode } from "react";
import type { Decision } from "../../protocol/decisions";
import { decision, decisionComment } from "../../test/decisionFixtures";
import DecisionReader from "./DecisionReader";
import type { DecisionReaderProps } from "./DecisionReader";
import type { NotifyCandidate } from "./publishPlan";

const NOW = Date.parse("2026-09-12T12:00:00Z");
const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;

const iso = (offsetMs: number): string => new Date(NOW + offsetMs).toISOString();

const BODY = [
  "## What I have",
  "",
  "- The 17+ listing went live at 09:10 and Apple has not flagged it.",
  "- Spend is capped at $40/day, so a bad day costs less than a lunch.",
  "- Rule 3 says no spend inside 48h of a listing change.",
  "",
  "Waiting until Monday costs the weekend traffic, which is the half of the week the",
  "creative was written for. Starting now breaks a rule you wrote for a reason.",
].join("\n");

const OPTIONS = [
  { key: "start", label: "Start today", description: "Spend begins tonight at the $40 cap." },
  {
    key: "hold",
    label: "Hold until the listing settles",
    description: "Resume Monday, once the 48h window closes.",
  },
  {
    key: "half",
    label: "Start at half the cap",
    description: "$20/day now, full spend once the window closes.",
  },
];

const CANDIDATES: readonly NotifyCandidate[] = [
  {
    botId: "b1",
    name: "auction",
    checked: true,
    locked: true,
    title: "The asking bot is always told",
  },
  { botId: "b2", name: "ledger", checked: true, locked: false, title: "Project lead" },
  { botId: "b9", name: "scout", checked: false, locked: false, title: "" },
];

const NAMES: Record<string, string> = { b1: "auction", b2: "ledger", b9: "scout" };

function Frame({ children }: { readonly children: ReactNode }): ReactElement {
  return (
    <div className="control-center" style={{ width: 760 }}>
      {children}
    </div>
  );
}

const noop = (): void => {};

function base(over: Partial<Decision>): DecisionReaderProps {
  return {
    decision: decision({
      body: BODY,
      options: OPTIONS,
      created_at: iso(-3 * HOUR),
      ...over,
    }),
    now: NOW,
    canControl: true,
    showBack: false,
    onBack: noop,
    showQuestion: true,
    onToggleQuestion: noop,
    picked: undefined,
    onPick: noop,
    projectName: () => "apple-ads",
    botName: (botId) => NAMES[botId],
    botAvatar: (botId) => ({ avatar: "", name: NAMES[botId] ?? botId }),
    candidates: CANDIDATES,
    chosen: new Set(["b1", "b2"]),
    onToggleNotify: noop,
    onEditDraft: noop,
    onOpenTray: noop,
    onResume: noop,
    onConfirm: noop,
    onDelete: noop,
  };
}

export const Open: Story = () => (
  <Frame>
    <DecisionReader
      {...base({
        priority: "urgent",
        deadline_at: iso(9 * HOUR),
        origin_chain: "b9,b1",
        on_behalf_of_bot_id: "b2",
        comments: [
          decisionComment({ created_at: iso(-2 * HOUR) }),
          decisionComment({
            id: "c2",
            author_kind: "bot",
            author_bot_id: "b1",
            author_name: "auction",
            body: "Caps are $40/day, hard stop. I can halve it if you want the weekend anyway.",
            created_at: iso(-1 * HOUR),
          }),
        ],
        comment_count: 2,
      })}
      picked="start"
    />
  </Frame>
);

export const Draft: Story = () => (
  <Frame>
    <DecisionReader
      {...base({
        state: "answered",
        ruling: {
          option: "half",
          text: "Start at half the cap today, full spend Monday.",
          answered_at: iso(-30 * 60 * 1000),
          answered_by: "owner",
        },
      })}
    />
  </Frame>
);

export const Held: Story = () => (
  <Frame>
    <DecisionReader {...base({ state: "held", held_until: iso(8 * DAY) })} />
  </Frame>
);

const SETTLED: Partial<Decision> = {
  state: "settled",
  published_at: iso(-3 * HOUR),
  created_at: iso(-6 * HOUR),
  ruling: {
    option: "start",
    text: "Start today. The cap is small enough that rule 3 can wait.",
    answered_at: iso(-3 * HOUR),
    answered_by: "owner",
  },
  notifications: [
    { bot_id: "b1", bot_name: "auction", created_at: iso(-3 * HOUR) },
    { bot_id: "b2", bot_name: "ledger", created_at: iso(-3 * HOUR) },
  ],
};

export const Settled: Story = () => (
  <Frame>
    <DecisionReader {...base(SETTLED)} showBack showQuestion={false} />
  </Frame>
);

export const SettledQuestionExpanded: Story = () => (
  <Frame>
    <DecisionReader {...base(SETTLED)} showBack />
  </Frame>
);

export const Relayed: Story = () => (
  <Frame>
    <DecisionReader
      {...base({
        state: "settled",
        published_at: iso(-3 * HOUR),
        created_at: iso(-6 * HOUR),
        ruling: {
          option: "start",
          text: "Start today. The cap is small enough that rule 3 can wait.",
          answered_at: iso(-3 * HOUR),
          answered_by: "owner-via-bot:b1",
        },
        notifications: [{ bot_id: "b1", bot_name: "auction", created_at: iso(-3 * HOUR) }],
      })}
    />
  </Frame>
);

export const Withdrawn: Story = () => (
  <Frame>
    <DecisionReader
      {...base({
        state: "withdrawn",
        withdrawn_reason: "Apple pulled the listing, so there is nothing to spend against.",
      })}
    />
  </Frame>
);

export const NoOptions: Story = () => (
  <Frame>
    <DecisionReader {...base({ options: [], recommendation: undefined })} />
  </Frame>
);

/** Real bots name options with words, not letters; the label must still read. */
export const LongOptions: Story = () => (
  <Frame>
    <DecisionReader
      {...base({
        recommendation: "by-host",
        options: [
          {
            key: "by-host",
            label:
              "One tag per machine (tower, janus, edge, hestia, auth, home-assistant, sydmini, fsn)",
            description:
              "Matches how you already talk about the fleet, so filing needs no judgement. Risk: cross-cutting work touches several hosts and ends up multi-tagged.",
          },
          {
            key: "by-concern",
            label:
              "One tag per concern (networking, storage, backups, ci, auth, media, monitoring)",
            description: "Cross-cutting work files cleanly and searches well later.",
          },
          { key: "both", label: "Both axes — host tags and concern tags" },
        ],
      })}
    />
  </Frame>
);
