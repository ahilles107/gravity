import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Dictation, DictationEvent } from "../../dictation";
import ChatComposer from "./ChatComposer";
import { appendSpoken } from "./useDictation";

/** A dictation source the test speaks through. */
function fakeDictation(available = true): {
  readonly dictation: Dictation;
  readonly say: (event: DictationEvent) => void;
  readonly stopped: () => boolean;
} {
  let listener: ((event: DictationEvent) => void) | null = null;
  let stopped = false;
  return {
    dictation: {
      available: async () => available,
      start: async (onEvent) => {
        listener = onEvent;
        return () => {
          stopped = true;
        };
      },
    },
    say: (event) => listener?.(event),
    stopped: () => stopped,
  };
}

function renderComposer(dictation: Dictation): void {
  render(
    <ChatComposer
      disabledReason={null}
      placeholder="Message"
      onSend={vi.fn<(text: string) => Promise<void>>()}
      dictation={dictation}
    />,
  );
}

describe("dictation in the composer", () => {
  it("writes what is said after what was typed, live", async () => {
    const voice = fakeDictation();
    renderComposer(voice.dictation);
    const field = screen.getByRole("textbox", { name: "Message" });
    await userEvent.type(field, "Please");
    await userEvent.click(await screen.findByRole("button", { name: "Dictate" }));
    expect(screen.getByText(/Listening/)).toBeInTheDocument();

    act(() => {
      voice.say({ kind: "partial", text: "sign the" });
    });
    expect(field).toHaveValue("Please sign the");
    act(() => {
      voice.say({ kind: "final", text: "sign the installer." });
    });
    expect(field).toHaveValue("Please sign the installer.");
    expect(screen.getByRole("button", { name: "Dictate" })).toBeInTheDocument();
  });

  it("stops on a second press and reports a refusal", async () => {
    const voice = fakeDictation();
    renderComposer(voice.dictation);
    await userEvent.click(await screen.findByRole("button", { name: "Dictate" }));
    await userEvent.click(screen.getByRole("button", { name: "Stop dictation" }));
    expect(voice.stopped()).toBe(true);

    await userEvent.click(screen.getByRole("button", { name: "Dictate" }));
    act(() => {
      voice.say({ kind: "ended", error: "Speech recognition is off for Gravity." });
    });
    expect(screen.getByText("Speech recognition is off for Gravity.")).toBeInTheDocument();
  });

  it("hides the mic where dictation is not available", async () => {
    renderComposer(fakeDictation(false).dictation);
    await Promise.resolve();
    expect(screen.queryByRole("button", { name: "Dictate" })).not.toBeInTheDocument();
  });

  it("joins spoken text to the draft with one space", () => {
    expect(appendSpoken("", "hello")).toBe("hello");
    expect(appendSpoken("hi", "there")).toBe("hi there");
    expect(appendSpoken("hi ", "there")).toBe("hi there");
    expect(appendSpoken("hi", "")).toBe("hi");
  });
});
