import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getPrefs, reloadPrefsForTest } from "../prefs";
import { stubLocalStorage, toastSpy } from "../test/spies";
import { useWindowShortcut } from "./useWindowShortcut";

const applyToggleWindowShortcut = vi.hoisted(() => vi.fn<(shortcut: string) => Promise<void>>());
vi.mock("../windowShortcut", () => ({
  applyToggleWindowShortcut,
  formatShortcut: (shortcut: string): string => shortcut,
}));

vi.mock("../analytics", () => ({
  captureException: vi.fn<(error: unknown, context: string) => void>(),
}));

describe("useWindowShortcut", () => {
  beforeEach(() => {
    applyToggleWindowShortcut.mockResolvedValue(undefined);
  });

  afterEach(() => {
    applyToggleWindowShortcut.mockReset();
  });

  it("registers the saved shortcut at launch", () => {
    stubLocalStorage({ "gravity.prefs": JSON.stringify({ toggleWindowShortcut: "Super+KeyG" }) });
    reloadPrefsForTest();
    const addToast = toastSpy();

    renderHook(() => {
      useWindowShortcut(addToast);
    });

    expect(applyToggleWindowShortcut).toHaveBeenCalledWith("Super+KeyG");
    expect(addToast).not.toHaveBeenCalled();
  });

  it("skips registration when no shortcut is set", () => {
    stubLocalStorage();
    reloadPrefsForTest();

    renderHook(() => {
      useWindowShortcut(toastSpy());
    });

    expect(applyToggleWindowShortcut).not.toHaveBeenCalled();
  });

  it("clears the preference and warns when the shell refuses it", async () => {
    stubLocalStorage({ "gravity.prefs": JSON.stringify({ toggleWindowShortcut: "Super+KeyG" }) });
    reloadPrefsForTest();
    applyToggleWindowShortcut.mockRejectedValue(new Error("taken"));
    const addToast = toastSpy();

    renderHook(() => {
      useWindowShortcut(addToast);
    });

    await waitFor(() => {
      expect(addToast).toHaveBeenCalledWith(
        "warn",
        "Shortcut unavailable",
        expect.stringContaining("could not be registered"),
      );
    });
    expect(getPrefs().toggleWindowShortcut).toBe("");
  });
});
