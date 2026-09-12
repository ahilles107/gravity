import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { decision } from "../../test/decisionFixtures";
import OptionList from "./OptionList";

describe("OptionList", () => {
  it("heading names what the asking bot recommends", () => {
    render(<OptionList decision={decision()} />);
    expect(screen.getByText("2 options · auction recommends start")).toBeInTheDocument();
    expect(screen.getByText("recommended")).toBeInTheDocument();
  });

  it("drops the recommendation from the heading when the bot did not make one", () => {
    render(<OptionList decision={decision({ recommendation: undefined })} />);
    expect(screen.getByText("2 options")).toBeInTheDocument();
    expect(screen.queryByText("recommended")).toBeNull();
  });

  it("hands the key back when a row is pressed", async () => {
    const onPick = vi.fn<(key: string) => void>();
    render(<OptionList decision={decision()} onPick={onPick} />);
    await userEvent.click(screen.getByText("Hold until the listing settles"));
    expect(onPick).toHaveBeenCalledWith("hold");
  });

  // Once it is ruled the rows are a record, not a choice: nothing to press,
  // no number caps, and the one that was taken says so.
  it("marks the chosen option and stops offering the rest once it is ruled", () => {
    const { container } = render(
      <OptionList
        decision={decision({
          state: "settled",
          ruling: {
            option: "hold",
            text: "Hold it.",
            answered_at: "2024-05-01T10:00:00.000Z",
            answered_by: "owner",
          },
        })}
      />,
    );
    expect(screen.getByText("Options offered")).toBeInTheDocument();
    expect(screen.getByText("chosen")).toBeInTheDocument();
    expect(screen.queryAllByRole("button")).toHaveLength(0);
    expect(container.querySelectorAll(".cc-option-num")).toHaveLength(0);
    expect(container.querySelectorAll(".cc-option-chosen")).toHaveLength(1);
    expect(container.querySelectorAll(".cc-option-passed")).toHaveLength(1);
  });

  it("renders nothing when the bot offered no options", () => {
    const { container } = render(<OptionList decision={decision({ options: [] })} />);
    expect(container).toBeEmptyDOMElement();
  });
});
