import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { voidSpy } from "../test/spies";
import CommandPalette from "./CommandPalette";
import type { PaletteAction } from "./CommandPalette";

function actions(runs: string[]): PaletteAction[] {
  return ["alice", "bob", "carol"].map((label) => ({
    id: `open-${label}`,
    label,
    hint: "open bot",
    run: () => {
      runs.push(label);
    },
  }));
}

function renderPalette() {
  const runs: string[] = [];
  const onSearch = vi.fn<(query: string) => void>();
  const onClose = voidSpy();
  render(<CommandPalette actions={actions(runs)} onSearch={onSearch} onClose={onClose} />);
  return { runs, onSearch, onClose, input: screen.getByRole("textbox") };
}

describe("CommandPalette", () => {
  it("focuses the input and lists every action", () => {
    const { input } = renderPalette();
    expect(input).toHaveFocus();
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
  });

  it("filters by fuzzy match and offers a search passthrough", async () => {
    const user = userEvent.setup();
    const { input, onSearch } = renderPalette();

    await user.type(input, "bo");
    expect(screen.getByText("bob")).toBeInTheDocument();
    expect(screen.queryByText("alice")).not.toBeInTheDocument();

    await user.click(screen.getByText("Search: bo"));
    expect(onSearch).toHaveBeenCalledWith("bo");
  });

  it("shows an empty state when nothing matches and only offers search", async () => {
    const user = userEvent.setup();
    const { input } = renderPalette();
    await user.type(input, "zzzz");
    expect(screen.getByText("Search: zzzz")).toBeInTheDocument();
    expect(screen.queryByText("alice")).not.toBeInTheDocument();
  });

  it("runs the highlighted action on Enter", async () => {
    const user = userEvent.setup();
    const { input, runs, onClose } = renderPalette();

    await user.keyboard("{ArrowDown}{Enter}");
    expect(runs).toEqual(["bob"]);
    expect(onClose).toHaveBeenCalled();
    expect(input).toBeInTheDocument();
  });

  it("wraps the highlight in both directions", async () => {
    const user = userEvent.setup();
    const { runs } = renderPalette();

    await user.keyboard("{ArrowUp}{Enter}");
    expect(runs).toEqual(["carol"]);
  });

  it("closes on Escape and on a backdrop click", async () => {
    const user = userEvent.setup();
    const { onClose } = renderPalette();

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("presentation"));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("does not close when the panel itself is clicked", async () => {
    const user = userEvent.setup();
    const { onClose } = renderPalette();
    await user.click(screen.getByRole("dialog"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("runs an action clicked with the mouse", async () => {
    const user = userEvent.setup();
    const { runs } = renderPalette();
    await user.click(screen.getByText("carol"));
    expect(runs).toEqual(["carol"]);
  });

  it("highlights the hovered option", async () => {
    const user = userEvent.setup();
    const { runs } = renderPalette();

    await user.hover(screen.getByText("carol"));
    await user.keyboard("{Enter}");
    expect(runs).toEqual(["carol"]);
  });

  it("renders an empty list without actions", () => {
    render(
      <CommandPalette actions={[]} onSearch={vi.fn<(q: string) => void>()} onClose={voidSpy()} />,
    );
    expect(screen.getByText("No matching commands.")).toBeInTheDocument();
  });
});
