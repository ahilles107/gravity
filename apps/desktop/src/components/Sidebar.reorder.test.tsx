import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as fx from "../test/fixtures";
import { renderSidebar } from "../test/sidebar";
import { stubLocalStorage } from "../test/spies";

afterEach(() => {
  vi.unstubAllGlobals();
});

const threeBots = [
  fx.bot({ id: "b1", name: "ann", sort_order: 0 }),
  fx.bot({ id: "b2", name: "bob", sort_order: 1 }),
  fx.bot({ id: "b3", name: "cat", sort_order: 2 }),
];

function row(name: string): HTMLElement {
  const button = screen.getByText(name).closest("button");
  if (button === null) {
    throw new Error(`no row for ${name}`);
  }
  return button;
}

async function openMenu(name: string): Promise<void> {
  await userEvent.setup().pointer({ keys: "[MouseRight]", target: screen.getByText(name) });
}

describe("Sidebar bot order", () => {
  it("offers only the moves that exist from the context menu", async () => {
    renderSidebar({ bots: threeBots });

    await openMenu("ann");
    expect(screen.queryByRole("menuitem", { name: "Move up" })).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Move down" })).toBeInTheDocument();
    await userEvent.setup().keyboard("{Escape}");

    await openMenu("cat");
    expect(screen.getByRole("menuitem", { name: "Move up" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Move down" })).not.toBeInTheDocument();
  });

  it("moves a bot up from the context menu", async () => {
    const user = userEvent.setup();
    const props = renderSidebar({ bots: threeBots });

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("cat") });
    await user.click(screen.getByRole("menuitem", { name: "Move up" }));

    expect(props.onReorderBots).toHaveBeenCalledWith("p1", ["b1", "b3", "b2"]);
  });

  it("offers no moves without the control grant", async () => {
    renderSidebar({ bots: threeBots, canControl: false });
    await openMenu("bob");
    expect(screen.queryByRole("menuitem", { name: "Move up" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Move down" })).not.toBeInTheDocument();
  });

  it("moves the focused row with Alt+Arrow keys", async () => {
    const user = userEvent.setup();
    const props = renderSidebar({ bots: threeBots });

    row("ann").focus();
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    expect(props.onReorderBots).not.toHaveBeenCalled();

    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    expect(props.onReorderBots).toHaveBeenCalledWith("p1", ["b2", "b1", "b3"]);
  });

  it("ignores plain arrow keys", async () => {
    const user = userEvent.setup();
    const props = renderSidebar({ bots: threeBots });
    row("ann").focus();
    await user.keyboard("{ArrowDown}");
    expect(props.onReorderBots).not.toHaveBeenCalled();
  });

  it("steps past the next plain row, not a pinned bot", async () => {
    stubLocalStorage({ "gravity.pinned-bots": JSON.stringify(["b2"]) });
    const user = userEvent.setup();
    const props = renderSidebar({ bots: threeBots });

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("ann") });
    await user.click(screen.getByRole("menuitem", { name: "Move down" }));

    expect(props.onReorderBots).toHaveBeenCalledWith("p1", ["b2", "b3", "b1"]);
  });

  it("drags a row below another", () => {
    const props = renderSidebar({ bots: threeBots });

    fireEvent.pointerDown(row("ann"), { button: 0, clientY: 0 });
    fireEvent.pointerMove(window, { clientY: 30 });
    fireEvent.pointerMove(row("cat"), { clientY: 30 });
    expect(row("cat")).toHaveClass("bot-row-drop-after");
    expect(row("ann")).toHaveClass("bot-row-dragging");
    fireEvent.pointerUp(window);

    expect(props.onReorderBots).toHaveBeenCalledWith("p1", ["b2", "b3", "b1"]);
    expect(row("cat")).not.toHaveClass("bot-row-drop-after");
  });

  it("treats a press without movement as a click", () => {
    const props = renderSidebar({ bots: threeBots });

    fireEvent.pointerDown(row("bob"), { button: 0, clientY: 0 });
    fireEvent.pointerMove(window, { clientY: 2 });
    fireEvent.pointerUp(window);
    fireEvent.click(row("bob"));

    expect(props.onReorderBots).not.toHaveBeenCalled();
    expect(props.onSelect).toHaveBeenCalledWith({ kind: "bot", botId: "b2" });
  });

  it("swallows the click that ends a drag", () => {
    const props = renderSidebar({ bots: threeBots });

    fireEvent.pointerDown(row("ann"), { button: 0, clientY: 0 });
    fireEvent.pointerMove(window, { clientY: 30 });
    fireEvent.pointerUp(window);
    fireEvent.click(row("ann"));

    expect(props.onSelect).not.toHaveBeenCalled();
    expect(props.onReorderBots).not.toHaveBeenCalled();
  });

  it("cancels a drag with Escape", () => {
    const props = renderSidebar({ bots: threeBots });

    fireEvent.pointerDown(row("ann"), { button: 0, clientY: 0 });
    fireEvent.pointerMove(window, { clientY: 30 });
    fireEvent.pointerMove(row("cat"), { clientY: 30 });
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.pointerUp(window);

    expect(props.onReorderBots).not.toHaveBeenCalled();
  });

  it("does not drag without the control grant", () => {
    const props = renderSidebar({ bots: threeBots, canControl: false });

    fireEvent.pointerDown(row("ann"), { button: 0, clientY: 0 });
    fireEvent.pointerMove(window, { clientY: 30 });
    fireEvent.pointerMove(row("cat"), { clientY: 30 });
    fireEvent.pointerUp(window);

    expect(props.onReorderBots).not.toHaveBeenCalled();
  });
});
