import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ChatMarkdown from "./ChatMarkdown";

describe("ChatMarkdown", () => {
  it("renders fenced code as a highlighted block and inline code inline", () => {
    render(
      <ChatMarkdown>
        {"Run `cargo test` first.\n\n```rust\nfn main() {}\n```\n\n```\nplain\nlines\n```"}
      </ChatMarkdown>,
    );
    expect(screen.getByText("cargo test")).toHaveClass("md-inline-code");
    expect(screen.getByText("fn")).toHaveClass("tok-keyword");
    expect(screen.getByText("rust")).toHaveClass("code-block-lang");
    expect(screen.getByText(/plain\s+lines/)).toBeInTheDocument();
    expect(screen.getByText("text")).toHaveClass("code-block-lang");
  });

  it("keeps links and images inert", () => {
    render(
      <ChatMarkdown>
        {"[click](javascript:alert(1)) ![shot](https://example.com/x.png)"}
      </ChatMarkdown>,
    );
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByText("click")).toHaveClass("markdown-link");
  });
});
