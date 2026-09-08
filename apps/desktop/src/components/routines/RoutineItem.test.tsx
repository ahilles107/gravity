import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import * as fx from "../../test/fixtures";
import RoutineItem from "./RoutineItem";

const noop = (): void => {};
const originals = {
  scrollHeight: Object.getOwnPropertyDescriptor(Element.prototype, "scrollHeight"),
  clientHeight: Object.getOwnPropertyDescriptor(Element.prototype, "clientHeight"),
};

function heightOf(el: Element, clamped: number, full: number): number {
  if (!el.classList.contains("routine-prompt")) {
    return 0;
  }
  return el.classList.contains("clamped") ? clamped : full;
}

// jsdom lays nothing out, so fake the overflow the clamp would produce in a browser.
function fakePromptOverflow(overflows: boolean): void {
  Object.defineProperty(Element.prototype, "clientHeight", {
    configurable: true,
    get(): number {
      return heightOf(this as Element, 100, 100);
    },
  });
  Object.defineProperty(Element.prototype, "scrollHeight", {
    configurable: true,
    get(): number {
      return heightOf(this as Element, overflows ? 400 : 100, 100);
    },
  });
}

function renderItem(prompt: string): void {
  render(
    <ul>
      <RoutineItem
        routine={fx.routine({ prompt })}
        bots={[fx.bot()]}
        connected
        canControl
        expanded={false}
        onToggleEnabled={noop}
        onRunNow={noop}
        onToggleHistory={noop}
      />
    </ul>,
  );
}

afterEach(() => {
  for (const [name, descriptor] of Object.entries(originals)) {
    if (descriptor !== undefined) {
      Object.defineProperty(Element.prototype, name, descriptor);
    }
  }
});

describe("RoutineItem", () => {
  it("leaves a short prompt clamped without a toggle", () => {
    fakePromptOverflow(false);
    renderItem("/weekly-report");
    expect(screen.getByText("/weekly-report")).toHaveClass("clamped");
    expect(screen.queryByRole("button", { name: "Show more" })).not.toBeInTheDocument();
  });

  it("expands and collapses a long prompt", async () => {
    const user = userEvent.setup();
    fakePromptOverflow(true);
    renderItem("line\n".repeat(40));

    const prompt = screen.getByText(/^line/);
    expect(prompt).toHaveClass("clamped");

    await user.click(screen.getByRole("button", { name: "Show more" }));
    expect(prompt).not.toHaveClass("clamped");

    await user.click(screen.getByRole("button", { name: "Show less" }));
    expect(prompt).toHaveClass("clamped");
  });
});
