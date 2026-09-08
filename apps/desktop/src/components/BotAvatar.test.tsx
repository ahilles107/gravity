import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import BotAvatar from "./BotAvatar";
import { BOT_ICONS } from "./botIcons";

function avatarOf(container: HTMLElement): HTMLElement {
  const node = container.querySelector(".bot-avatar");
  if (node === null) {
    throw new Error("no avatar rendered");
  }
  return node as HTMLElement;
}

describe("BotAvatar", () => {
  it("renders an icon avatar as its bundled image", () => {
    const { container } = render(<BotAvatar avatar="icon:orbit" name="alice" id="b1" />);
    const node = avatarOf(container);
    expect(node.tagName).toBe("IMG");
    expect(node.getAttribute("src")).toBe(BOT_ICONS.orbit);
  });

  /** A client older than the daemon must not render a broken image. */
  it("falls back to a swatch for an icon it does not ship", () => {
    const { container } = render(<BotAvatar avatar="icon:banana" name="alice" id="b1" />);
    const node = avatarOf(container);
    expect(node.tagName).toBe("SPAN");
    expect(node.textContent).toBe("A");
  });

  it("renders a colour avatar as an initial on that colour", () => {
    const { container } = render(<BotAvatar avatar="color:#4a90d9" name="alice" id="b1" />);
    const node = avatarOf(container);
    expect(node.textContent).toBe("A");
    expect(node.style.background).toBe("rgb(74, 144, 217)");
  });

  it("falls back to a colour derived from the id, stable across renders", () => {
    const first = render(<BotAvatar avatar="" name="alice" id="b1" />);
    const background = avatarOf(first.container).style.background;
    expect(background.length).toBeGreaterThan(0);

    const second = render(<BotAvatar avatar="" name="alice" id="b1" />);
    expect(avatarOf(second.container).style.background).toBe(background);
  });

  it("gives different bots different fallback colours", () => {
    const a = render(<BotAvatar avatar="" name="alice" id="b1" />);
    const b = render(<BotAvatar avatar="" name="bob" id="b2" />);
    expect(avatarOf(a.container).style.background).not.toBe(avatarOf(b.container).style.background);
  });

  /** Avatars are decorative; the bot name is always rendered beside them. */
  it("is hidden from assistive technology", () => {
    const { container } = render(<BotAvatar avatar="icon:orbit" name="alice" id="b1" />);
    expect(avatarOf(container).getAttribute("aria-hidden")).toBe("true");
  });

  it("copes with a nameless bot", () => {
    const { container } = render(<BotAvatar avatar="" name="  " id="b1" />);
    expect(avatarOf(container).textContent).toBe("?");
  });
});
