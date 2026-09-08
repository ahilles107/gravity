import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getPrefs, reloadPrefsForTest } from "../../prefs";
import { stubLocalStorage } from "../../test/spies";
import type * as windowShortcut from "../../windowShortcut";
import ShortcutSettings from "./ShortcutSettings";

const applyToggleWindowShortcut = vi.hoisted(() => vi.fn<(shortcut: string) => Promise<void>>());
vi.mock("../../windowShortcut", async (importOriginal) => {
  const actual = await importOriginal<typeof windowShortcut>();
  return { ...actual, applyToggleWindowShortcut };
});

function recorder(): HTMLElement {
  return screen.getByRole("button", { name: "Show or hide window shortcut" });
}

describe("ShortcutSettings", () => {
  beforeEach(() => {
    stubLocalStorage();
    reloadPrefsForTest();
    applyToggleWindowShortcut.mockResolvedValue(undefined);
  });

  afterEach(() => {
    applyToggleWindowShortcut.mockReset();
  });

  it("records a modifier combination and registers it with the shell", async () => {
    const user = userEvent.setup();
    render(<ShortcutSettings />);
    expect(recorder()).toHaveTextContent("Not set");

    await user.click(recorder());
    expect(recorder()).toHaveTextContent("Press keys…");
    await user.keyboard("{Meta>}{Shift>}g{/Shift}{/Meta}");

    await waitFor(() => {
      expect(getPrefs().toggleWindowShortcut).toBe("Super+Shift+KeyG");
    });
    expect(applyToggleWindowShortcut).toHaveBeenCalledWith("Super+Shift+KeyG");
    expect(recorder()).not.toHaveTextContent("Press keys…");
  });

  it("ignores a bare key while recording and backs out on Escape", async () => {
    const user = userEvent.setup();
    render(<ShortcutSettings />);

    await user.click(recorder());
    await user.keyboard("g");
    expect(recorder()).toHaveTextContent("Press keys…");

    await user.keyboard("{Escape}");
    expect(recorder()).toHaveTextContent("Not set");
    expect(applyToggleWindowShortcut).not.toHaveBeenCalled();
  });

  it("explains why a single-modifier combination cannot be bound", async () => {
    const user = userEvent.setup();
    render(<ShortcutSettings />);

    await user.click(recorder());
    await user.keyboard("{Meta>}c{/Meta}");

    expect(await screen.findByText(/That combination is not available/)).toBeInTheDocument();
    expect(recorder()).toHaveTextContent("Press keys…");
    expect(applyToggleWindowShortcut).not.toHaveBeenCalled();
  });

  it("clears the shortcut from the Clear button or Backspace", async () => {
    stubLocalStorage({
      "gravity.prefs": JSON.stringify({ toggleWindowShortcut: "Super+Shift+KeyG" }),
    });
    reloadPrefsForTest();
    const user = userEvent.setup();
    render(<ShortcutSettings />);

    await user.click(screen.getByRole("button", { name: "Clear" }));
    await waitFor(() => {
      expect(getPrefs().toggleWindowShortcut).toBe("");
    });
    expect(applyToggleWindowShortcut).toHaveBeenLastCalledWith("");
    expect(screen.queryByRole("button", { name: "Clear" })).not.toBeInTheDocument();

    await user.click(recorder());
    await user.keyboard("{Control>}{Alt>}g{/Alt}{/Control}");
    await waitFor(() => {
      expect(getPrefs().toggleWindowShortcut).toBe("Ctrl+Alt+KeyG");
    });
    await user.click(recorder());
    await user.keyboard("{Backspace}");
    await waitFor(() => {
      expect(getPrefs().toggleWindowShortcut).toBe("");
    });
  });

  it("keeps the old shortcut and shows the refusal when the shell rejects", async () => {
    stubLocalStorage({
      "gravity.prefs": JSON.stringify({ toggleWindowShortcut: "Super+Shift+KeyG" }),
    });
    reloadPrefsForTest();
    applyToggleWindowShortcut.mockRejectedValue(new Error("already in use"));
    const user = userEvent.setup();
    render(<ShortcutSettings />);

    await user.click(recorder());
    await user.keyboard("{Control>}{Alt>}g{/Alt}{/Control}");

    expect(await screen.findByText(/could not be registered: already in use/)).toBeInTheDocument();
    expect(getPrefs().toggleWindowShortcut).toBe("Super+Shift+KeyG");
  });
});
