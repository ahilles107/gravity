import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Decision } from "../../protocol/decisions";
import { decision } from "../../test/decisionFixtures";
import DecisionReader from "./DecisionReader";
import type { DecisionReaderProps } from "./DecisionReader";
import type { NotifyCandidate } from "./publishPlan";

const NOW = Date.parse("2026-09-12T12:00:00Z");
const HOUR = 60 * 60 * 1000;

const iso = (offsetMs: number): string => new Date(NOW + offsetMs).toISOString();

const NAMES: Record<string, string> = { b1: "auction", b2: "ledger", b9: "scout" };

const CANDIDATES: readonly NotifyCandidate[] = [
  {
    botId: "b1",
    name: "auction",
    checked: true,
    locked: true,
    title: "The asking bot is always told",
  },
  { botId: "b2", name: "ledger", checked: true, locked: false, title: "Project lead" },
];

const noop = (): void => {};

function props(
  over: Partial<Decision>,
  propsOver: Partial<DecisionReaderProps> = {},
): DecisionReaderProps {
  return {
    decision: decision({ created_at: iso(-3 * HOUR), ...over }),
    now: NOW,
    canControl: true,
    showBack: false,
    onBack: noop,
    showQuestion: true,
    onToggleQuestion: noop,
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
    ...propsOver,
  };
}

const SETTLED: Partial<Decision> = {
  state: "settled",
  created_at: iso(-6 * HOUR),
  published_at: iso(-3 * HOUR),
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

describe("DecisionReader", () => {
  it("says who is waiting, who they are waiting for and where it happened", () => {
    const { container } = render(
      <DecisionReader {...props({ on_behalf_of_bot_id: "b2", origin_chain: "b9,b2,b1" })} />,
    );
    const byline = container.querySelector(".cc-byline")?.textContent ?? "";
    expect(byline).toContain("for ledger");
    expect(byline).toContain("via scout → ledger");
    expect(byline).toContain("apple-ads");
    expect(byline).toContain("raised 3h ago");
  });

  it("names a bot that is gone rather than leaving a gap", () => {
    const { container } = render(<DecisionReader {...props({ on_behalf_of_bot_id: "b7" })} />);
    expect(container.querySelector(".cc-byline")?.textContent).toContain("for a deleted bot");
  });

  it("flags urgency and the closing window while it is still the owner's to answer", () => {
    render(<DecisionReader {...props({ priority: "urgent", deadline_at: iso(9 * HOUR) })} />);
    expect(screen.getByText("Urgent")).toBeInTheDocument();
    expect(screen.getByText(/Choice closes in 9 hours/)).toBeInTheDocument();
  });

  // Once it is ruled the deadline is history; shouting about it would be noise.
  it("drops the urgency and the deadline once it is ruled", () => {
    render(
      <DecisionReader {...props({ ...SETTLED, priority: "urgent", deadline_at: iso(9 * HOUR) })} />,
    );
    expect(screen.queryByText("Urgent")).toBeNull();
    expect(screen.queryByText(/Choice closes/)).toBeNull();
  });

  it("leads with the ruling and folds the question away until it is asked for", async () => {
    const onToggleQuestion = vi.fn<() => void>();
    const settled = props(SETTLED, { showQuestion: false, onToggleQuestion });
    const { rerender } = render(<DecisionReader {...settled} />);

    expect(screen.getByText(/Start today. The cap is small enough/)).toBeInTheDocument();
    expect(screen.queryByText(/The 17\+ listing is live/)).toBeNull();

    await userEvent.click(screen.getByText("The question as auction put it"));
    expect(onToggleQuestion).toHaveBeenCalledTimes(1);

    rerender(<DecisionReader {...settled} showQuestion />);
    expect(screen.getByText(/The 17\+ listing is live/)).toBeInTheDocument();
  });

  it("names the bots that were told and how long it stayed open", () => {
    const { container } = render(<DecisionReader {...props(SETTLED)} />);
    const foot = container.querySelector(".cc-card-foot")?.textContent ?? "";
    expect(foot).toContain("Told");
    expect(foot).toContain("auction");
    expect(foot).toContain("ledger");
    expect(screen.getByText("open 3h")).toBeInTheDocument();
  });

  it("shows a draft as words nothing has seen yet, with its own actions", async () => {
    const onEditDraft = vi.fn<() => void>();
    const onOpenTray = vi.fn<() => void>();
    const onToggleNotify = vi.fn<(botId: string) => void>();
    render(
      <DecisionReader
        {...props(
          {
            state: "answered",
            ruling: {
              option: "hold",
              text: "Hold it until the listing settles.",
              answered_at: iso(-HOUR),
              answered_by: "owner",
            },
          },
          { onEditDraft, onOpenTray, onToggleNotify },
        )}
      />,
    );

    expect(screen.getByText(/Hold it until the listing settles/)).toBeInTheDocument();
    expect(screen.getByText("bots can't see this yet")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(onEditDraft).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole("button", { name: "Publish…" }));
    expect(onOpenTray).toHaveBeenCalledTimes(1);

    // The asking bot is always told, so its chip is not the owner's to switch off.
    const asker = screen.getByRole("button", { name: "auction" });
    expect(asker).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(asker);
    expect(onToggleNotify).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "ledger" }));
    expect(onToggleNotify).toHaveBeenCalledWith("b2");
  });

  it("offers to take a held decision up again", async () => {
    const onResume = vi.fn<() => void>();
    render(
      <DecisionReader
        {...props({ state: "held", held_until: iso(8 * 24 * HOUR) }, { onResume })}
      />,
    );
    expect(screen.getByText(/was told/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Take it up now" }));
    expect(onResume).toHaveBeenCalledTimes(1);
  });

  // A ruling a bot filed on the owner's behalf is a claim about what they said.
  it("asks the owner to confirm a ruling a bot relayed", async () => {
    const onConfirm = vi.fn<() => void>();
    render(
      <DecisionReader
        {...props(
          {
            ...SETTLED,
            ruling: {
              option: "start",
              text: "Start today.",
              answered_at: iso(-3 * HOUR),
              answered_by: "owner-via-bot:b1",
            },
          },
          { onConfirm },
        )}
      />,
    );
    expect(
      screen.getByText("Recorded by auction as something you said at its terminal."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Confirm this is what I said" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("does not offer the confirmation on a ruling the owner typed", () => {
    render(<DecisionReader {...props(SETTLED)} />);
    expect(screen.queryByRole("button", { name: "Confirm this is what I said" })).toBeNull();
  });

  it("offers to delete the record, and only to someone who may", async () => {
    const onDelete = vi.fn<() => void>();
    const { rerender } = render(<DecisionReader {...props({}, { onDelete })} />);
    await userEvent.click(screen.getByRole("button", { name: "Delete record" }));
    expect(onDelete).toHaveBeenCalledTimes(1);

    rerender(<DecisionReader {...props({}, { onDelete })} canControl={false} />);
    expect(screen.queryByRole("button", { name: "Delete record" })).toBeNull();
  });
});
