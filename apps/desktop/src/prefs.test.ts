import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_PREFS, getPrefs, reloadPrefsForTest, subscribePrefs, updatePrefs } from "./prefs";
import { stubLocalStorage } from "./test/spies";

describe("prefs", () => {
  beforeEach(() => {
    stubLocalStorage();
    reloadPrefsForTest();
  });

  it("starts from the defaults", () => {
    expect(getPrefs()).toEqual(DEFAULT_PREFS);
  });

  it("persists an update and notifies subscribers", () => {
    const listener = vi.fn<() => void>();
    subscribePrefs(listener);

    updatePrefs({ terminalFontSize: 16 });

    expect(getPrefs().terminalFontSize).toBe(16);
    expect(listener).toHaveBeenCalledOnce();

    reloadPrefsForTest();
    expect(getPrefs().terminalFontSize).toBe(16);
  });

  it("does not notify subscribers when an update changes nothing", () => {
    const listener = vi.fn<() => void>();
    subscribePrefs(listener);

    updatePrefs({ terminalFontSize: DEFAULT_PREFS.terminalFontSize });

    expect(listener).not.toHaveBeenCalled();
  });

  it("rejects an out-of-range font size back to the default", () => {
    updatePrefs({ terminalFontSize: 90 });
    expect(getPrefs().terminalFontSize).toBe(DEFAULT_PREFS.terminalFontSize);
  });

  it("accepts any string as the window shortcut and drops other types", () => {
    updatePrefs({ toggleWindowShortcut: "Super+Shift+KeyG" });
    expect(getPrefs().toggleWindowShortcut).toBe("Super+Shift+KeyG");

    stubLocalStorage({ "gravity.prefs": JSON.stringify({ toggleWindowShortcut: 7 }) });
    reloadPrefsForTest();
    expect(getPrefs().toggleWindowShortcut).toBe("");
  });

  it("survives malformed stored json", () => {
    stubLocalStorage({ "gravity.prefs": "{nope" });
    reloadPrefsForTest();
    expect(getPrefs()).toEqual(DEFAULT_PREFS);
  });

  it("keeps unrelated fields when patching one", () => {
    updatePrefs({ dockBadge: false });
    updatePrefs({ terminalFontFamily: "Menlo" });
    expect(getPrefs()).toEqual({
      ...DEFAULT_PREFS,
      dockBadge: false,
      terminalFontFamily: "Menlo",
    });
  });
});
