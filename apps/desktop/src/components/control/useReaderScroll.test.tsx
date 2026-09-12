import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import { forgetControlSession } from "./controlSession";
import { useReaderScroll } from "./useReaderScroll";

beforeEach(forgetControlSession);

function Pane({ decisionId }: { readonly decisionId: string | undefined }): ReactElement {
  const ref = useReaderScroll(decisionId);
  return <div data-testid="scroll" ref={ref} />;
}

function mount(decisionId: string | undefined) {
  const view = render(<Pane decisionId={decisionId} />);
  const node = view.getByTestId("scroll");
  return { view, node };
}

describe("useReaderScroll", () => {
  it("starts a never-scrolled decision at the top", () => {
    const { view, node } = mount("a");
    node.scrollTop = 120;

    view.rerender(<Pane decisionId="b" />);

    expect(node.scrollTop).toBe(0);
  });

  it("restores where a decision was left", () => {
    const { view, node } = mount("a");
    node.scrollTop = 120;

    view.rerender(<Pane decisionId="b" />);
    node.scrollTop = 40;
    view.rerender(<Pane decisionId="a" />);

    expect(node.scrollTop).toBe(120);

    view.rerender(<Pane decisionId="b" />);
    expect(node.scrollTop).toBe(40);
  });

  it("leaves the offset alone with nothing selected", () => {
    const { view, node } = mount("a");
    node.scrollTop = 90;

    view.rerender(<Pane decisionId={undefined} />);
    view.rerender(<Pane decisionId="a" />);

    expect(node.scrollTop).toBe(90);
  });
});
