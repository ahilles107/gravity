import { describe, expect, it } from "vitest";
import { highlight, languageOf } from "./highlight";

function kinds(code: string, language: string): string[] {
  return highlight(code, language)
    .filter((token) => token.kind !== "plain")
    .map((token) => `${token.kind}:${token.text}`);
}

describe("highlight", () => {
  it("marks keywords, strings, numbers and comments", () => {
    expect(kinds('const x = "hi"; // note\nreturn 42;', "ts")).toEqual([
      "keyword:const",
      'string:"hi"',
      "comment:// note",
      "keyword:return",
      "number:42",
    ]);
    expect(kinds("def f(): # why\n  return None", "py")).toEqual([
      "keyword:def",
      "comment:# why",
      "keyword:return",
      "keyword:None",
    ]);
    expect(kinds("SELECT 1 -- one", "sql")).toEqual([
      "keyword:SELECT",
      "number:1",
      "comment:-- one",
    ]);
    expect(kinds('{"a": true}', "json")).toEqual(['string:"a"', "keyword:true"]);
  });

  it("keeps the text intact and leaves unknown languages plain", () => {
    const code = 'fn main() {\n    println!("x");\n}';
    expect(
      highlight(code, "rs")
        .map((t) => t.text)
        .join(""),
    ).toBe(code);
    expect(highlight("whatever", "brainfuck")).toEqual([{ kind: "plain", text: "whatever" }]);
  });

  it("finds a language from a file name or fence", () => {
    expect(languageOf("src/app.tsx")).toBe("tsx");
    expect(languageOf("Rust")).toBe("rust");
    expect(languageOf("notes.docx")).toBe("");
  });
});
