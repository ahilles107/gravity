import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { applyToggleWindowShortcut, formatShortcut, shortcutFromKeys } from "./windowShortcut";
import type { ShortcutKeys } from "./windowShortcut";

const invoke = vi.hoisted(() => vi.fn<(command: string, args?: unknown) => Promise<unknown>>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

function keys(overrides: Partial<ShortcutKeys>): ShortcutKeys {
  return {
    code: "KeyG",
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...overrides,
  };
}

describe("shortcutFromKeys", () => {
  it("joins modifiers and the physical key in the plugin's order", () => {
    expect(shortcutFromKeys(keys({ metaKey: true, shiftKey: true }))).toEqual({
      ok: true,
      shortcut: "Super+Shift+KeyG",
    });
    expect(shortcutFromKeys(keys({ ctrlKey: true, altKey: true, code: "Digit1" }))).toEqual({
      ok: true,
      shortcut: "Ctrl+Alt+Digit1",
    });
  });

  it("stays pending on a lone modifier", () => {
    expect(shortcutFromKeys(keys({ metaKey: true, code: "MetaLeft" }))).toEqual({
      ok: false,
      reason: "pending",
    });
    expect(shortcutFromKeys(keys({ metaKey: true, code: "" }))).toEqual({
      ok: false,
      reason: "pending",
    });
  });

  it("refuses a press the OS needs or that is too easy to hit", () => {
    // Bare keys, and Shift alone, would swallow ordinary typing everywhere.
    expect(shortcutFromKeys(keys({}))).toEqual({ ok: false, reason: "refused" });
    expect(shortcutFromKeys(keys({ shiftKey: true }))).toEqual({ ok: false, reason: "refused" });
    // A single modifier would hijack copy or quit in every app.
    expect(shortcutFromKeys(keys({ metaKey: true, code: "KeyC" }))).toEqual({
      ok: false,
      reason: "refused",
    });
    expect(shortcutFromKeys(keys({ metaKey: true, code: "Space" }))).toEqual({
      ok: false,
      reason: "refused",
    });
    // App switching, force quit and log out stay with the OS.
    expect(shortcutFromKeys(keys({ metaKey: true, shiftKey: true, code: "Tab" }))).toEqual({
      ok: false,
      reason: "refused",
    });
    expect(shortcutFromKeys(keys({ metaKey: true, altKey: true, code: "Escape" }))).toEqual({
      ok: false,
      reason: "refused",
    });
    expect(shortcutFromKeys(keys({ metaKey: true, shiftKey: true, code: "KeyQ" }))).toEqual({
      ok: false,
      reason: "refused",
    });
  });
});

describe("formatShortcut", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("uses macOS glyphs on a Mac", () => {
    vi.stubGlobal("navigator", { userAgentData: { platform: "macOS" }, userAgent: "" });
    expect(formatShortcut("Super+Shift+KeyG")).toBe("⌘⇧G");
    expect(formatShortcut("Ctrl+Alt+ArrowUp")).toBe("⌃⌥Up");
  });

  it("falls back to the user agent when UA data is missing", () => {
    vi.stubGlobal("navigator", { userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)" });
    expect(formatShortcut("Super+KeyG")).toBe("⌘G");
  });

  it("spells modifiers out elsewhere", () => {
    vi.stubGlobal("navigator", { userAgentData: { platform: "Windows" }, userAgent: "" });
    expect(formatShortcut("Super+Shift+Digit1")).toBe("Win+Shift+1");
    expect(formatShortcut("Ctrl+Numpad5")).toBe("Ctrl+Num 5");
    expect(formatShortcut("")).toBe("");
  });
});

describe("applyToggleWindowShortcut", () => {
  beforeEach(() => {
    invoke.mockResolvedValue(null);
  });

  afterEach(() => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    invoke.mockReset();
  });

  it("does nothing outside the Tauri shell", async () => {
    await applyToggleWindowShortcut("Super+Shift+KeyG");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("hands the accelerator to the shell and surfaces a refusal", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    await applyToggleWindowShortcut("Super+Shift+KeyG");
    expect(invoke).toHaveBeenCalledWith("set_toggle_window_shortcut", {
      shortcut: "Super+Shift+KeyG",
    });

    invoke.mockRejectedValue(new Error("already taken"));
    await expect(applyToggleWindowShortcut("Super+Shift+KeyG")).rejects.toThrow("already taken");
  });
});
