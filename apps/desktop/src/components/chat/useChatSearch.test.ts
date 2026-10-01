import { describe, expect, it } from "vitest";
import { step, text, turn } from "../../test/chatFixtures";
import { matchTurns } from "./useChatSearch";

describe("matchTurns", () => {
  const turns = [
    turn({ id: "a", items: [text("Built the installer.")] }),
    turn({ id: "b", items: [step({ title: "Edited update.rs" })] }),
    turn({
      id: "c",
      trigger: { kind: "bus", from: "lead", msg_kind: "task", num: 1, text: "sign it" },
      items: [],
    }),
  ];

  it("finds turns by their text, steps and trigger", () => {
    expect(matchTurns(turns, "INSTALLER").map((t) => t.id)).toEqual(["a"]);
    expect(matchTurns(turns, "update.rs").map((t) => t.id)).toEqual(["b"]);
    expect(matchTurns(turns, "lead").map((t) => t.id)).toEqual(["c"]);
    expect(matchTurns(turns, "  ")).toBe(turns);
  });
});
