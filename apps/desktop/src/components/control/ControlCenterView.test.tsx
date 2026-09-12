import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Decision } from "../../protocol/decisions";
import type { NotifyLevel } from "../../protocol/entities";
import * as dfx from "../../test/decisionFixtures";
import { FakeDaemon } from "../../test/fakeDaemon";
import * as fx from "../../test/fixtures";
import ControlCenterView from "./ControlCenterView";
import { forgetControlSession } from "./controlSession";

beforeEach(forgetControlSession);

const NOW = Date.parse("2026-09-12T12:00:00Z");
const RAISED = new Date(NOW - 5 * 3_600_000).toISOString();

interface Seeded {
  readonly daemon: FakeDaemon;
  /** Answers a decision request from the current record and keeps the result. */
  readonly mutate: (
    type: "answer_decision" | "unanswer_decision" | "hold_decision",
    change: (current: Decision) => Decision,
  ) => Seeded;
}

/** A daemon whose `get_decision` returns what the last mutation produced. */
function seed(decisions: readonly Decision[]): Seeded {
  const store = new Map(decisions.map((item) => [item.id, item]));
  const daemon = new FakeDaemon()
    .onRequest("list_decisions", (body) => ({
      type: "decisions",
      req_id: "1",
      decisions: [...store.values()].filter((item) => {
        if (body.type !== "list_decisions") {
          return false;
        }
        if (body.state === "open") {
          return item.state === "open" || item.state === "answered";
        }
        return item.state === body.state;
      }),
    }))
    .onRequest("get_decision", (body) => ({
      type: "decision",
      req_id: "1",
      decision:
        (body.type === "get_decision" ? store.get(body.decision_id) : undefined) ?? dfx.decision(),
    }))
    .onRequest("list_tags", () => ({ type: "tags", req_id: "1", tags: [dfx.tag()] }));
  const seeded: Seeded = {
    daemon,
    mutate: (type, change) => {
      daemon.onRequest(type, (body) => {
        const id = body.type === type ? body.decision_id : "";
        const next = change(store.get(id) ?? dfx.decision());
        store.set(next.id, next);
        return { type: "decision", req_id: "1", decision: next };
      });
      return seeded;
    },
  };
  return seeded;
}

function renderView(seeded: Seeded, over: { canControl?: boolean; decisionId?: string } = {}) {
  const { daemon } = seeded;
  const onToast = vi.fn<(level: NotifyLevel, title: string, body: string) => void>();
  vi.spyOn(Date, "now").mockReturnValue(NOW);
  const view = render(
    <ControlCenterView
      client={daemon}
      projects={[fx.project({ id: "p1", name: "Orbit", lead_bot_id: "b9" })]}
      bots={[
        fx.bot({ id: "b1", name: "auction", project_id: "p1" }),
        fx.bot({ id: "b9", name: "chief", project_id: "p1" }),
      ]}
      connected
      canControl={over.canControl ?? true}
      decisionId={over.decisionId}
      onToast={onToast}
    />,
  );
  return { onToast, view };
}

const open = (over: Partial<Decision> = {}): Decision =>
  dfx.decision({ created_at: RAISED, ...over });

describe("ControlCenterView", () => {
  it("lists what is waiting, urgent first, with the deadline in words", async () => {
    renderView(
      seed([
        open({ id: "d1", title: "Later one" }),
        open({
          id: "d2",
          title: "Urgent one",
          priority: "urgent",
          deadline_at: new Date(NOW + 6 * 3_600_000).toISOString(),
        }),
      ]),
    );
    const rows = await screen.findAllByRole("button", { name: /one/ });
    expect(rows[0]).toHaveTextContent("Urgent");
    expect(rows[0]).toHaveTextContent("Urgent one");
    expect(rows[0]).toHaveTextContent("6h left");
    expect(rows[0]).toHaveTextContent("raised 5h ago");
    expect(screen.getByText("2 waiting on you · 1 urgent")).toBeInTheDocument();
  });

  it("saves a picked option on its own, with its label as the words", async () => {
    const seeded = seed([open()]).mutate("answer_decision", (current) => ({
      ...current,
      state: "answered",
      ruling: { option: "start", text: "Start today", answered_at: RAISED, answered_by: "owner" },
    }));
    const { daemon } = seeded;
    renderView(seeded);
    await screen.findByRole("button", { name: /Save ruling/ });
    await userEvent.keyboard("1");
    await userEvent.keyboard("{Meta>}{Enter}{/Meta}");
    await waitFor(() => {
      expect(daemon.requests.map((item) => item.body)).toContainEqual({
        type: "answer_decision",
        decision_id: "d1",
        ruling_text: "Start today",
        ruling_option: "start",
      });
    });
  });

  it("saves a ruling once there are words, with the picked option", async () => {
    const seeded = seed([open()]).mutate("answer_decision", (current) => ({
      ...current,
      state: "answered",
      ruling: { option: "start", text: "Let it fire.", answered_at: RAISED, answered_by: "owner" },
    }));
    const { daemon } = seeded;
    renderView(seeded);
    const save = await screen.findByRole("button", { name: /Save ruling/ });
    expect(save).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: /Start today/ }));
    await userEvent.type(screen.getByLabelText("Your ruling"), "Let it fire.");
    expect(save).toBeEnabled();
    await userEvent.click(save);
    await waitFor(() => {
      expect(daemon.requests.map((item) => item.body)).toContainEqual({
        type: "answer_decision",
        decision_id: "d1",
        ruling_text: "Let it fire.",
        ruling_option: "start",
      });
    });
    // The draft is provisional and the header offers to publish it.
    expect(await screen.findByText("bots can't see this yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Publish 1/ })).toBeInTheDocument();
  });

  it("publishes every draft from the tray with the asker always told", async () => {
    const draft = open({
      state: "answered",
      ruling: { text: "Let it fire.", answered_at: RAISED, answered_by: "owner" },
    });
    const seeded = seed([draft]);
    const { daemon } = seeded;
    daemon.onRequest("publish_decisions", () => ({
      type: "publish_result",
      req_id: "1",
      results: [{ decision_id: "d1", notified: ["auction"], skipped: [] }],
    }));
    renderView(seeded);
    await userEvent.click(await screen.findByRole("button", { name: /Publish 1/ }));
    const tray = screen.getByText("Ready to publish").closest(".cc-tray");
    if (!(tray instanceof HTMLElement)) {
      throw new Error("no tray");
    }
    // Unticking the lead leaves the asker, whose chip cannot be turned off.
    await userEvent.click(within(tray).getByRole("button", { name: "chief" }));
    await userEvent.click(within(tray).getByRole("button", { name: "auction" }));
    await userEvent.click(within(tray).getByRole("button", { name: "Publish 1 ruling" }));
    await waitFor(() => {
      expect(daemon.requests.map((item) => item.body)).toContainEqual({
        type: "publish_decisions",
        items: [{ decision_id: "d1", notify_bot_ids: ["b1"] }],
      });
    });
  });

  it("editing a draft discards it on the daemon and puts the words back", async () => {
    const draft = open({
      state: "answered",
      ruling: { option: "start", text: "Let it fire.", answered_at: RAISED, answered_by: "owner" },
    });
    renderView(
      seed([draft]).mutate("unanswer_decision", (current) => ({
        ...current,
        state: "open",
        ruling: undefined,
      })),
    );
    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));
    await waitFor(() => {
      expect(screen.getByLabelText("Your ruling")).toHaveValue("Let it fire.");
    });
    expect(screen.getByText("Start today", { selector: ".cc-pick-chip" })).toBeInTheDocument();
  });

  it("holds with a date and a note, then moves to the next decision", async () => {
    const seeded = seed([
      open({ id: "d1", title: "First" }),
      open({ id: "d2", title: "Second" }),
    ]).mutate("hold_decision", (current) => ({
      ...current,
      state: "held",
      held_until: "2026-09-20T00:00:00Z",
    }));
    const { daemon } = seeded;
    renderView(seeded);
    await screen.findByRole("heading", { name: "First" });
    await userEvent.click(screen.getByRole("button", { name: "Later" }));
    await userEvent.type(screen.getByLabelText("Hold until"), "2026-09-20");
    await userEvent.type(
      screen.getByPlaceholderText(/What you want to know first/),
      "What does hourly cost?",
    );
    await userEvent.click(screen.getByRole("button", { name: "Hold" }));
    await waitFor(() => {
      const hold = daemon.requests.find((item) => item.body.type === "hold_decision")?.body;
      expect(hold).toMatchObject({ decision_id: "d1", comment: "What does hourly cost?" });
      expect(hold).toHaveProperty("until", expect.stringContaining("2026-09-"));
    });
    expect(await screen.findByRole("heading", { name: "Second" })).toBeInTheDocument();
    // The held one folds away and comes back on request.
    const held = screen.getByRole("button", { name: /Held/ });
    expect(screen.queryByRole("button", { name: /^First/ })).not.toBeInTheDocument();
    await userEvent.click(held);
    expect(screen.getByRole("button", { name: /First/ })).toHaveTextContent("until 20 Sep");
  });

  it("moves with the keyboard and picks options with digits", async () => {
    renderView(seed([open({ id: "d1", title: "First" }), open({ id: "d2", title: "Second" })]));
    await screen.findByRole("heading", { name: "First" });
    await userEvent.keyboard("j");
    expect(screen.getByRole("heading", { name: "Second" })).toBeInTheDocument();
    await userEvent.keyboard("1");
    expect(screen.getByText("Start today", { selector: ".cc-pick-chip" })).toBeInTheDocument();
    await userEvent.keyboard("1");
    expect(
      screen.queryByText("Start today", { selector: ".cc-pick-chip" }),
    ).not.toBeInTheDocument();
    await userEvent.keyboard("k");
    expect(screen.getByRole("heading", { name: "First" })).toBeInTheDocument();
  });

  it("comes back to the record it was reading after a bot is opened", async () => {
    const decisions = [open({ id: "d1", title: "First" }), open({ id: "d2", title: "Second" })];
    const { view } = renderView(seed(decisions));
    await userEvent.click(await screen.findByRole("button", { name: /Second/ }));
    expect(screen.getByRole("heading", { name: "Second" })).toBeInTheDocument();

    view.unmount();
    renderView(seed(decisions));

    expect(await screen.findByRole("heading", { name: "Second" })).toBeInTheDocument();
  });

  it("comes back to the registry, its search and its open ruling", async () => {
    const settled = open({
      state: "settled",
      title: "Widen retention?",
      ruling: { text: "Go to 90.", answered_at: RAISED, answered_by: "owner" },
      published_at: RAISED,
      notifications: [{ bot_id: "b1", bot_name: "auction", created_at: RAISED }],
    });
    const { view } = renderView(seed([settled]));
    await userEvent.click(await screen.findByRole("button", { name: /Settled/ }));
    await userEvent.type(screen.getByLabelText("Search rulings"), "auction");
    await userEvent.click(screen.getByRole("button", { name: /Go to 90/ }));
    await screen.findByRole("heading", { name: "Widen retention?" });

    view.unmount();
    renderView(seed([settled]));

    expect(await screen.findByRole("heading", { name: "Widen retention?" })).toBeInTheDocument();
    // Back from the reader lands on the ledger with the search still in place.
    await userEvent.keyboard("{Escape}");
    expect(await screen.findByLabelText("Search rulings")).toHaveValue("auction");
  });

  it("says when nothing is waiting", async () => {
    const settled = open({
      state: "settled",
      title: "Widen retention?",
      ruling: { text: "Go to 90.", answered_at: RAISED, answered_by: "owner" },
      published_at: new Date(NOW - 3 * 3_600_000).toISOString(),
    });
    renderView(seed([settled]));
    expect(await screen.findByText("Nothing is waiting on you.")).toBeInTheDocument();
    expect(screen.getByText("1 rulings in the registry · ⌘2")).toBeInTheDocument();
  });

  it("walks the ledger with the arrows and opens the highlighted ruling with ↩", async () => {
    const ruled = (id: string, title: string, text: string): Decision =>
      open({
        id,
        title,
        state: "settled",
        published_at: RAISED,
        ruling: { text, answered_at: RAISED, answered_by: "owner" },
      });
    renderView(seed([ruled("d1", "Widen retention?", "Go to 90."), ruled("d2", "Pin it?", "No.")]));
    await userEvent.click(await screen.findByRole("button", { name: /Settled/ }));

    await userEvent.keyboard("{ArrowDown}");
    const first = screen.getByRole("button", { name: /Go to 90/ });
    expect(first).toHaveAttribute("aria-current", "true");
    // Highlighting is not opening: the ledger is still on screen.
    expect(screen.getByLabelText("Search rulings")).toBeInTheDocument();

    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("button", { name: /No\./ })).toHaveAttribute("aria-current", "true");

    await userEvent.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "Pin it?" })).toBeInTheDocument();

    // Esc puts the cursor back where the reader was opened from.
    await userEvent.keyboard("{Escape}");
    expect(await screen.findByRole("button", { name: /No\./ })).toHaveAttribute(
      "aria-current",
      "true",
    );
  });

  it("opens a settled ruling from the registry and Esc returns to the ledger", async () => {
    const settled = open({
      state: "settled",
      title: "Widen retention?",
      ruling: { option: "start", text: "Go to 90.", answered_at: RAISED, answered_by: "owner" },
      published_at: RAISED,
      notifications: [{ bot_id: "b1", bot_name: "auction", created_at: RAISED }],
    });
    renderView(seed([settled]));
    await userEvent.click(await screen.findByRole("button", { name: /Settled/ }));
    await userEvent.type(screen.getByLabelText("Search rulings"), "auction");
    await userEvent.click(screen.getByRole("button", { name: /Go to 90/ }));
    expect(await screen.findByText("Told")).toBeInTheDocument();
    expect(screen.queryByLabelText("Your ruling")).not.toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(await screen.findByLabelText("Search rulings")).toBeInTheDocument();
  });

  it("asks in the thread with ⇧⌘↩ and shows the question at once", async () => {
    const { daemon } = seed([open({ comments: [] })]);
    daemon.onRequest("comment_decision", () => ({
      type: "decision_comment",
      req_id: "1",
      comment: dfx.decisionComment({ body: "Is the cap a hard stop?" }),
    }));
    renderView({ daemon, mutate: () => seed([]) });
    await userEvent.type(await screen.findByLabelText("Your ruling"), "Is the cap a hard stop?");
    await userEvent.keyboard("{Meta>}{Shift>}{Enter}{/Shift}{/Meta}");
    await waitFor(() => {
      expect(daemon.requests.map((item) => item.body)).toContainEqual({
        type: "comment_decision",
        decision_id: "d1",
        body: "Is the cap a hard stop?",
      });
    });
    expect(await screen.findByText("Is the cap a hard stop?")).toBeInTheDocument();
    expect(screen.getByLabelText("Your ruling")).toHaveValue("");
  });

  it("keeps a read-only device looking, not ruling", async () => {
    renderView(seed([open()]), { canControl: false });
    expect(await screen.findByLabelText("Your ruling")).toBeDisabled();
    expect(screen.getByRole("button", { name: /Save ruling/ })).toBeDisabled();
  });

  it("appends a bot's answer to the thread as it arrives", async () => {
    const { daemon } = seed([open({ comments: [] })]);
    renderView({ daemon, mutate: () => seed([]) });
    await screen.findByRole("heading", { name: "Waive rule 3 and start the ads today?" });
    act(() => {
      daemon.emit("decision_comment_new", {
        type: "decision_comment_new",
        comment: dfx.decisionComment({
          author_kind: "bot",
          author_bot_id: "b1",
          author_name: "auction",
          body: "Hard stop, not a soft alert.",
        }),
      });
    });
    expect(await screen.findByText("Hard stop, not a soft alert.")).toBeInTheDocument();
  });
});
