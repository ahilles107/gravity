import { describe, expect, it } from "vitest";
import { botTabs } from "./BotTabs";

describe("botTabs", () => {
  it("offers the browser where the daemon serves one", () => {
    expect(botTabs({ chat: true }, false)).toEqual(["chat", "terminal", "routines"]);
    expect(botTabs({ chat: true, browser: true }, false)).toEqual([
      "chat",
      "terminal",
      "browser",
      "routines",
    ]);
    expect(botTabs({ chat: false }, false)).toEqual(["terminal", "routines"]);
  });

  it("gives a linked bot its terminal when the daemon relays it", () => {
    expect(botTabs({ chat: true, browser: true }, true)).toEqual(["chat"]);
    expect(botTabs({ chat: true, browser: true, peerTerminal: true }, true)).toEqual([
      "chat",
      "terminal",
    ]);
  });
});
