import { fireEvent, renderHook } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import * as dfx from "../../test/decisionFixtures";
import { NO_FILTER } from "./decisions";
import { navListFor, useControlKeys } from "./useControlKeys";
import type { ControlKeyDeps } from "./useControlKeys";

function deps(over: Partial<ControlKeyDeps> = {}): ControlKeyDeps {
  return {
    tab: "waiting",
    setTab: vi.fn<ControlKeyDeps["setTab"]>(),
    navList: ["d1", "d2", "d3"],
    cursorId: "d2",
    moveCursor: vi.fn<ControlKeyDeps["moveCursor"]>(),
    select: vi.fn<ControlKeyDeps["select"]>(),
    reading: dfx.decision({ id: "d2" }),
    readingOpen: true,
    back: vi.fn<ControlKeyDeps["back"]>(),
    trayOpen: false,
    setTrayOpen: vi.fn<ControlKeyDeps["setTrayOpen"]>(),
    hasDrafts: true,
    holdOpen: false,
    setHoldOpen: vi.fn<ControlKeyDeps["setHoldOpen"]>(),
    togglePick: vi.fn<ControlKeyDeps["togglePick"]>(),
    saveRuling: vi.fn<ControlKeyDeps["saveRuling"]>(),
    askInThread: vi.fn<ControlKeyDeps["askInThread"]>(),
    textareaRef: createRef<HTMLTextAreaElement>(),
    searchRef: createRef<HTMLInputElement>(),
    ...over,
  };
}

let unmountLast: (() => void) | undefined;

/** One listener at a time: the previous hook is unmounted so it cannot answer first. */
function mount(over: Partial<ControlKeyDeps> = {}): ControlKeyDeps {
  unmountLast?.();
  const d = deps(over);
  unmountLast = renderHook(() => useControlKeys(d)).unmount;
  return d;
}

const press = (key: string, init: KeyboardEventInit = {}): void => {
  fireEvent.keyDown(window, { key, ...init });
};

describe("useControlKeys", () => {
  it("moves through the list with arrows and j/k, stopping at the ends", () => {
    const d = mount();
    press("ArrowDown");
    expect(d.moveCursor).toHaveBeenLastCalledWith("d3");
    press("k");
    expect(d.moveCursor).toHaveBeenLastCalledWith("d1");
    mount({ cursorId: "d3", moveCursor: d.moveCursor });
    press("j");
    expect(d.moveCursor).toHaveBeenLastCalledWith("d3");
  });

  // Arrows only move the cursor; opening is ↩, so the ledger does not tear
  // the reading pane open on every row passed over.
  it("starts from the top when nothing is highlighted, and ↩ opens the row", () => {
    const d = mount({ cursorId: undefined });
    press("ArrowUp");
    expect(d.moveCursor).toHaveBeenLastCalledWith("d1");
    expect(d.select).not.toHaveBeenCalled();
    const e = mount({ cursorId: "d2" });
    press("Enter");
    expect(e.select).toHaveBeenLastCalledWith("d2");
  });

  it("saves with ⌘↩ and asks with ⇧⌘↩, even while typing", () => {
    const d = mount();
    const field = document.createElement("textarea");
    document.body.append(field);
    fireEvent.keyDown(field, { key: "Enter", metaKey: true });
    expect(d.saveRuling).toHaveBeenCalledOnce();
    fireEvent.keyDown(field, { key: "Enter", metaKey: true, shiftKey: true });
    expect(d.askInThread).toHaveBeenCalledOnce();
    field.remove();
  });

  it("toggles the tray with ⌘⇧P only when there are drafts", () => {
    const d = mount();
    press("p", { metaKey: true, shiftKey: true });
    expect(d.setTrayOpen).toHaveBeenCalledWith(true);
    const e = mount({ hasDrafts: false });
    press("P", { metaKey: true, shiftKey: true });
    expect(e.setTrayOpen).not.toHaveBeenCalled();
  });

  it("switches tabs with ⌘1 ⌘2 ⌘3 and leaves other chords alone", () => {
    const d = mount();
    press("2", { metaKey: true });
    expect(d.setTab).toHaveBeenLastCalledWith("settled");
    press("3", { ctrlKey: true });
    expect(d.setTab).toHaveBeenLastCalledWith("tags");
    press("k", { metaKey: true });
    expect(d.moveCursor).not.toHaveBeenCalled();
  });

  it("picks options with digits and reaches the textarea and hold strip by letter", () => {
    const d = mount();
    press("1");
    expect(d.togglePick).toHaveBeenCalledWith("start");
    press("8");
    expect(d.togglePick).toHaveBeenCalledOnce();
    press("h");
    expect(d.setHoldOpen).toHaveBeenCalledWith(true);
    const area = document.createElement("textarea");
    document.body.append(area);
    const e = mount({ textareaRef: { current: area } });
    press("r");
    expect(document.activeElement).toBe(area);
    expect(e.togglePick).not.toHaveBeenCalled();
    area.remove();
  });

  it("ignores composer keys on a decision that is not open, and letters while typing", () => {
    const d = mount({ reading: dfx.decision({ id: "d2", state: "settled" }) });
    press("1");
    press("h");
    expect(d.togglePick).not.toHaveBeenCalled();
    expect(d.setHoldOpen).not.toHaveBeenCalled();
    const input = document.createElement("input");
    document.body.append(input);
    const e = mount();
    fireEvent.keyDown(input, { key: "j" });
    fireEvent.keyDown(input, { key: "1" });
    expect(e.moveCursor).not.toHaveBeenCalled();
    expect(e.togglePick).not.toHaveBeenCalled();
    input.remove();
  });

  it("Esc leaves a field, then closes the tray, then the hold strip, then goes back", () => {
    const input = document.createElement("input");
    document.body.append(input);
    input.focus();
    const a = mount({ trayOpen: true });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(document.activeElement).not.toBe(input);
    expect(a.setTrayOpen).not.toHaveBeenCalled();
    input.remove();
    press("Escape");
    expect(a.setTrayOpen).toHaveBeenCalledWith(false);
    const b = mount({ holdOpen: true });
    press("Escape");
    expect(b.setHoldOpen).toHaveBeenCalledWith(false);
    const c = mount({ tab: "settled" });
    press("Escape");
    expect(c.back).toHaveBeenCalledOnce();
    const w = mount({ tab: "waiting" });
    press("Escape");
    expect(w.back).not.toHaveBeenCalled();
  });

  it("focuses the registry search with / on the Settled tab only", () => {
    const search = document.createElement("input");
    document.body.append(search);
    mount({ tab: "settled", searchRef: { current: search } });
    press("/");
    expect(document.activeElement).toBe(search);
    search.blur();
    mount({ tab: "waiting", searchRef: { current: search } });
    press("/");
    expect(document.activeElement).not.toBe(search);
    search.remove();
  });
});

describe("navListFor", () => {
  const lists = {
    waiting: [dfx.decision({ id: "w1" })],
    held: [dfx.decision({ id: "h1", state: "held" })],
    registry: [dfx.decision({ id: "s1", state: "settled" })],
  };

  it("lists waiting rows, then held rows once the group is open", () => {
    expect(navListFor("waiting", lists, false)).toEqual(["w1"]);
    expect(navListFor("waiting", lists, true)).toEqual(["w1", "h1"]);
  });

  it("lists the ledger on Settled and nothing on Tags", () => {
    expect(navListFor("settled", lists, false)).toEqual(["s1"]);
    expect(navListFor("tags", lists, true)).toEqual([]);
  });

  it("leaves out ledger rows the project chip hides", () => {
    const filter = { ...NO_FILTER, projectFilter: "p2" };
    expect(navListFor("settled", lists, false, filter)).toEqual([]);
  });

  // Arrows that visit rows the search has hidden would jump the cursor off screen.
  it("leaves out ledger rows the search and tag filter hide", () => {
    const filter = { ...NO_FILTER, query: "nothing like it" };
    expect(navListFor("settled", lists, false, filter)).toEqual([]);
  });
});
