import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import Markdown from "./Markdown";

describe("Markdown", () => {
  it("renders the structure a bot wrote", () => {
    const { container } = render(<Markdown>{"## Evidence\n\n- one\n- two"}</Markdown>);
    expect(screen.getByRole("heading", { name: "Evidence" })).toBeInTheDocument();
    expect(container.querySelectorAll("li")).toHaveLength(2);
  });

  // A decision body is model output rendered into the owner's window.
  it("does not pass raw HTML through", () => {
    const { container } = render(
      <Markdown>{'<img src=x onerror="alert(1)"><b>bold</b>'}</Markdown>,
    );
    // Escaped into visible text rather than parsed into elements.
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(container.innerHTML).toContain("&lt;img");
  });

  it("renders links as inert text rather than something to click", () => {
    const { container } = render(<Markdown>{"[click me](javascript:alert(1))"}</Markdown>);
    expect(container.querySelector("a")).toBeNull();
    expect(screen.getByText("click me")).toBeInTheDocument();
  });

  // A remote src is fetched on render, so it tells whoever hosts it exactly
  // when the owner opened the decision. Nothing downstream stops it: the Tauri
  // config sets no CSP.
  it("does not fetch images a bot points at", () => {
    const { container } = render(
      <Markdown>{"![the graph](https://example.invalid/pixel.png)"}</Markdown>,
    );
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("the graph")).toBeInTheDocument();
  });

  it("still says an image was there when it had no alt text", () => {
    render(<Markdown>{"![](https://example.invalid/pixel.png)"}</Markdown>);
    expect(screen.getByText("image")).toBeInTheDocument();
  });
});
