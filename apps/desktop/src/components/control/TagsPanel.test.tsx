import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Tag } from "../../protocol/decisions";
import * as dfx from "../../test/decisionFixtures";
import TagsPanel from "./TagsPanel";

const TAGS: readonly Tag[] = [
  dfx.tag({
    id: "t1",
    name: "spend",
    description: "money leaving the account",
    uses: { p1: 2, p2: 1 },
    open_uses: 1,
    last_used_at: "2026-09-10T10:00:00Z",
  }),
  dfx.tag({ id: "t2", name: "backups", description: "", uses: {}, open_uses: 0 }),
  dfx.tag({ id: "t3", name: "gone", retired_at: "2026-08-01T00:00:00Z" }),
];

/** The rows are sorted by name, so `backups` is 0 and `spend` is 1. */
function deleteButton(index: number): HTMLElement {
  const found = screen.getAllByRole("button", { name: "Delete" })[index];
  if (found === undefined) {
    throw new Error(`no Delete button at ${index}`);
  }
  return found;
}

function renderPanel(over: { canControl?: boolean } = {}) {
  const onAdd = vi.fn<(name: string) => Promise<boolean>>().mockResolvedValue(true);
  const onRename = vi.fn<(name: string, to: string) => Promise<boolean>>().mockResolvedValue(true);
  const onDescribe = vi
    .fn<(name: string, description: string) => Promise<boolean>>()
    .mockResolvedValue(true);
  const onDelete = vi.fn<(name: string) => Promise<boolean>>().mockResolvedValue(true);
  render(
    <div className="control-center">
      <TagsPanel
        tags={TAGS}
        canControl={over.canControl ?? true}
        onAdd={onAdd}
        onRename={onRename}
        onDescribe={onDescribe}
        onDelete={onDelete}
      />
    </div>,
  );
  return { onAdd, onRename, onDescribe, onDelete, user: userEvent.setup() };
}

describe("TagsPanel", () => {
  it("lists the live tags with what they cost to keep", () => {
    renderPanel();
    expect(screen.getByText("2 tags")).toBeInTheDocument();
    expect(screen.getByLabelText("Tag spend")).toHaveValue("spend");
    expect(screen.getByText("3 decisions")).toBeInTheDocument();
    expect(screen.getByText("last used 10 Sep")).toBeInTheDocument();
    expect(screen.getByText("unused")).toBeInTheDocument();
    expect(screen.queryByLabelText("Tag gone")).not.toBeInTheDocument();
  });

  // "Apple Ads" and "apple-ads" have to be the same tag or the taxonomy splits.
  it("normalises a new tag to something a bot can type back", async () => {
    const { onAdd, user } = renderPanel();
    await user.type(screen.getByLabelText("New tag"), "Apple Ads");
    await user.click(screen.getByRole("button", { name: "Add tag" }));
    expect(onAdd).toHaveBeenCalledWith("apple-ads");
  });

  it("refuses a name a bot could not file under", async () => {
    const { user } = renderPanel();
    await user.type(screen.getByLabelText("New tag"), "spend!");
    expect(screen.getByRole("button", { name: "Add tag" })).toBeDisabled();
  });

  it("renames on blur", async () => {
    const { onRename, user } = renderPanel();
    const input = screen.getByLabelText("Tag spend");
    await user.clear(input);
    await user.type(input, "money");
    await user.tab();
    expect(onRename).toHaveBeenCalledWith("spend", "money");
  });

  it("puts the old name back when the rename does not land", async () => {
    const { onRename, user } = renderPanel();
    onRename.mockResolvedValue(false);
    const input = screen.getByLabelText("Tag spend");
    await user.clear(input);
    await user.type(input, "money");
    await user.tab();
    expect(input).toHaveValue("spend");
  });

  it("says what a description is for and saves it on blur", async () => {
    const { onDescribe, user } = renderPanel();
    const input = screen.getByLabelText("Description of backups");
    expect(input).toHaveAttribute("placeholder", "What it means, in a sentence bots can read");
    await user.type(input, "nightly copies");
    await user.tab();
    expect(onDescribe).toHaveBeenCalledWith("backups", "nightly copies");
  });

  // Deleting reaches every decision, so it says how many are still open first.
  it("asks before deleting, naming what it would touch", async () => {
    const { onDelete, user } = renderPanel();
    await user.click(deleteButton(1));
    expect(
      screen.getByText(/Remove from 3 decisions \(1 open\)\? This cannot be undone\./),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Keep it" }));
    expect(screen.getByLabelText("Tag spend")).toBeInTheDocument();
    expect(onDelete).not.toHaveBeenCalled();

    await user.click(deleteButton(1));
    await user.click(screen.getByRole("button", { name: "Delete tag" }));
    expect(onDelete).toHaveBeenCalledWith("spend");
  });

  it("is read-only without control", () => {
    renderPanel({ canControl: false });
    expect(screen.getByLabelText("New tag")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add tag" })).toBeDisabled();
    expect(screen.getByLabelText("Tag spend")).toBeDisabled();
    for (const button of screen.getAllByRole("button", { name: "Delete" })) {
      expect(button).toBeDisabled();
    }
  });
});
