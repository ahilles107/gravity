import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { decision } from "../../test/decisionFixtures";
import WaitingList from "./WaitingList";

const NOW = Date.parse("2026-09-12T12:00:00Z");

const urgent = decision({
  id: "d1",
  title: "Waive rule 3",
  priority: "urgent",
  deadline_at: "2026-09-12T21:00:00Z",
  created_at: "2026-09-12T06:00:00Z",
});

const draft = decision({
  id: "d2",
  title: "Move the cap",
  state: "answered",
  created_at: "2026-09-11T09:00:00Z",
});

const held = decision({
  id: "d5",
  title: "Sign the CDN contract",
  state: "held",
  held_until: "2026-09-19T09:00:00Z",
});

type SelectSpy = ReturnType<typeof vi.fn<(decisionId: string) => void>>;

function renderList(over: Partial<Parameters<typeof WaitingList>[0]> = {}): {
  readonly onSelect: SelectSpy;
} {
  const onSelect = vi.fn<(decisionId: string) => void>();
  render(
    <WaitingList
      waiting={[urgent, draft]}
      held={[held]}
      settledCount={12}
      loaded
      selectedId="d1"
      heldOpen={false}
      onToggleHeld={() => {}}
      onSelect={onSelect}
      now={NOW}
      projectName={() => "Acme"}
      botName={() => "alice"}
      {...over}
    />,
  );
  return { onSelect };
}

describe("WaitingList", () => {
  it("counts the urgent and the unpublished in the summary", () => {
    renderList();
    expect(
      screen.getByText("2 waiting on you · 1 urgent · 1 answered, unpublished"),
    ).toBeInTheDocument();
  });

  it("marks what is urgent and how long it has", () => {
    renderList();
    expect(screen.getByText("Urgent")).toBeInTheDocument();
    expect(screen.getByText("9h left")).toBeInTheDocument();
    expect(screen.getByText("Draft")).toBeInTheDocument();
  });

  it("keeps held decisions folded away until asked for", async () => {
    const user = userEvent.setup();
    const onToggleHeld = vi.fn<() => void>();
    const { rerender } = render(
      <WaitingList
        waiting={[urgent]}
        held={[held]}
        settledCount={0}
        loaded
        heldOpen={false}
        onToggleHeld={onToggleHeld}
        onSelect={() => {}}
        now={NOW}
        projectName={() => "Acme"}
        botName={() => "alice"}
      />,
    );

    expect(screen.queryByText("Sign the CDN contract")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { expanded: false }));
    expect(onToggleHeld).toHaveBeenCalled();

    rerender(
      <WaitingList
        waiting={[urgent]}
        held={[held]}
        settledCount={0}
        loaded
        heldOpen
        onToggleHeld={onToggleHeld}
        onSelect={() => {}}
        now={NOW}
        projectName={() => "Acme"}
        botName={() => "alice"}
      />,
    );
    expect(screen.getByText("Sign the CDN contract")).toBeInTheDocument();
    expect(screen.getByText("until 19 Sep")).toBeInTheDocument();
  });

  it("shows the empty state when nothing is waiting", () => {
    renderList({ waiting: [], held: [] });

    expect(screen.getByText("Nothing is waiting on you.")).toBeInTheDocument();
    expect(screen.getByText("Bots will raise the next decision here.")).toBeInTheDocument();
    expect(screen.getByText("12 rulings in the registry · ⌘2")).toBeInTheDocument();
  });

  it("selects the row that was clicked", async () => {
    const user = userEvent.setup();
    const { onSelect } = renderList();
    await user.click(screen.getByText("Move the cap"));
    expect(onSelect).toHaveBeenCalledWith("d2");
  });
});
