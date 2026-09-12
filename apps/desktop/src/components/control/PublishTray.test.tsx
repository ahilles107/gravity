import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { decision } from "../../test/decisionFixtures";
import PublishTray from "./PublishTray";
import type { NotifyCandidate } from "./publishPlan";

const first = decision({
  id: "d1",
  title: "Waive rule 3",
  state: "answered",
  ruling: {
    option: "start",
    text: "Start today, cap the spend.",
    answered_at: "2026-09-12T10:00:00Z",
    answered_by: "owner",
  },
});

const second = decision({
  id: "d2",
  title: "Move the cap",
  options: [],
  state: "answered",
  ruling: { text: "Yes, 4 TB.", answered_at: "2026-09-12T11:00:00Z", answered_by: "owner" },
});

const candidates: readonly NotifyCandidate[] = [
  {
    botId: "b1",
    name: "auction",
    checked: true,
    locked: true,
    title: "The asking bot is always told",
  },
];

interface Spies {
  readonly onEdit: ReturnType<typeof vi.fn<(decisionId: string) => void>>;
  readonly onPublish: ReturnType<typeof vi.fn<() => void>>;
  readonly onClose: ReturnType<typeof vi.fn<() => void>>;
}

function renderTray(over: Partial<Parameters<typeof PublishTray>[0]> = {}): Spies {
  const spies: Spies = {
    onEdit: vi.fn<(decisionId: string) => void>(),
    onPublish: vi.fn<() => void>(),
    onClose: vi.fn<() => void>(),
  };
  render(
    <PublishTray
      drafts={[first, second]}
      candidatesFor={() => candidates}
      chosenFor={() => new Set(["b1"])}
      onToggle={() => {}}
      busy={false}
      canControl
      {...spies}
      {...over}
    />,
  );
  return spies;
}

describe("PublishTray", () => {
  it("shows every draft with the words that will be quoted", () => {
    renderTray();
    expect(screen.getByText("Waive rule 3")).toBeInTheDocument();
    expect(screen.getByText("start · Start today")).toBeInTheDocument();
    expect(screen.getByText("“Start today, cap the spend.”")).toBeInTheDocument();
    expect(screen.getByText("“Yes, 4 TB.”")).toBeInTheDocument();
  });

  it("counts the rulings on the button", () => {
    renderTray();
    expect(screen.getByRole("button", { name: "Publish 2 rulings" })).toBeInTheDocument();
    screen.getByRole("button", { name: "Not yet" });

    renderTray({ drafts: [first] });
    expect(screen.getByRole("button", { name: "Publish 1 ruling" })).toBeInTheDocument();
  });

  it("closes without publishing", async () => {
    const user = userEvent.setup();
    const spies = renderTray();
    await user.click(screen.getByRole("button", { name: "Not yet" }));
    expect(spies.onClose).toHaveBeenCalled();
    expect(spies.onPublish).not.toHaveBeenCalled();
  });

  it("edits the draft it was asked about", async () => {
    const user = userEvent.setup();
    const spies = renderTray();
    await user.click(screen.getAllByRole("button", { name: "Edit" })[1]);
    expect(spies.onEdit).toHaveBeenCalledWith("d2");
  });

  it("cannot publish twice while one is in flight", () => {
    renderTray({ busy: true });
    expect(screen.getByRole("button", { name: "Publish 2 rulings" })).toBeDisabled();
  });
});
