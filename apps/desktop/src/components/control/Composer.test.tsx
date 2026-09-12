import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef } from "react";
import type { ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";
import { decision } from "../../test/decisionFixtures";
import Composer from "./Composer";
import { useComposer } from "./useComposer";

const subject = decision({ id: "d1" });

interface HarnessProps {
  readonly canControl?: boolean;
  readonly busy?: boolean;
  readonly onSave: () => void;
  readonly onAsk: () => void;
  readonly onHold: () => void;
}

/** The real composer state, so the toggles behave as they do in the view. */
function Harness({
  canControl = true,
  busy = false,
  onSave,
  onAsk,
  onHold,
}: HarnessProps): ReactElement {
  const composer = useComposer("d1");
  const ref = useRef<HTMLTextAreaElement>(null);
  return (
    <>
      <button type="button" onClick={() => composer.togglePick("start")}>
        pick start
      </button>
      <Composer
        decision={subject}
        composer={composer}
        canControl={canControl}
        busy={busy}
        textareaRef={ref}
        onSave={onSave}
        onAsk={onAsk}
        onHold={onHold}
      />
    </>
  );
}

interface Spies {
  readonly onSave: ReturnType<typeof vi.fn<() => void>>;
  readonly onAsk: ReturnType<typeof vi.fn<() => void>>;
  readonly onHold: ReturnType<typeof vi.fn<() => void>>;
}

function renderComposer(over: Partial<HarnessProps> = {}): Spies {
  const spies: Spies = {
    onSave: vi.fn<() => void>(),
    onAsk: vi.fn<() => void>(),
    onHold: vi.fn<() => void>(),
  };
  render(<Harness {...spies} {...over} />);
  return spies;
}

describe("Composer", () => {
  it("will not save an empty ruling, but an option alone will do", async () => {
    const user = userEvent.setup();
    renderComposer();
    expect(screen.getByRole("button", { name: /Save ruling/ })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "pick start" }));
    expect(screen.getByRole("button", { name: /Save ruling/ })).toBeEnabled();
    // Asking still needs a question.
    expect(screen.getByRole("button", { name: "Ask in thread" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "pick start" }));
    expect(screen.getByRole("button", { name: /Save ruling/ })).toBeDisabled();
    await user.type(screen.getByLabelText("Your ruling"), "Start today.");
    expect(screen.getByRole("button", { name: /Save ruling/ })).toBeEnabled();
  });

  it("hands the words to the view", async () => {
    const user = userEvent.setup();
    const spies = renderComposer();
    await user.type(screen.getByLabelText("Your ruling"), "Start today.");

    await user.click(screen.getByRole("button", { name: "Ask in thread" }));
    expect(spies.onAsk).toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: /Save ruling/ }));
    expect(spies.onSave).toHaveBeenCalled();
  });

  it("opens the hold strip from Later", async () => {
    const user = userEvent.setup();
    const spies = renderComposer();
    expect(screen.queryByLabelText("Hold until")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Later" }));
    await user.click(screen.getByRole("button", { name: "Hold" }));
    expect(spies.onHold).toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Later" }));
    expect(screen.queryByLabelText("Hold until")).not.toBeInTheDocument();
  });

  it("shows the picked option and clears it", async () => {
    const user = userEvent.setup();
    renderComposer();
    await user.click(screen.getByRole("button", { name: "pick start" }));

    expect(screen.getByText("Start today")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "clear" }));
    expect(screen.queryByText("Start today")).not.toBeInTheDocument();
  });

  it("says what the words are for, picked or not", async () => {
    const user = userEvent.setup();
    renderComposer();
    expect(screen.getByLabelText("Your ruling")).toHaveAttribute(
      "placeholder",
      "auction will quote this verbatim. Pick an option, write, or both.",
    );

    await user.click(screen.getByRole("button", { name: "pick start" }));
    expect(screen.getByLabelText("Your ruling")).toHaveAttribute(
      "placeholder",
      "Why start, or any conditions (optional). auction will quote this verbatim.",
    );
  });

  it("is inert for a read-only client", () => {
    renderComposer({ canControl: false });
    expect(screen.getByLabelText("Your ruling")).toBeDisabled();
    expect(screen.getByRole("button", { name: /Save ruling/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Ask in thread" })).toBeDisabled();
  });
});
