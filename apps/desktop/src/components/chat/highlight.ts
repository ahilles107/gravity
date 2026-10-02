// A small syntax highlighter for code in chat and file previews: comments,
// strings, numbers and keywords for the languages bots write most. It is not
// a parser; it only has to make code easier to scan than plain monospace.

type TokenKind = "plain" | "keyword" | "string" | "comment" | "number";

export interface Token {
  readonly kind: TokenKind;
  readonly text: string;
}

type Family = "slash" | "hash" | "sql" | "json";

/** Beyond this, highlighting costs more than it helps. */
const MAX_HIGHLIGHT_CHARS = 100_000;

const FAMILY: Readonly<Record<string, Family>> = {
  ts: "slash",
  tsx: "slash",
  typescript: "slash",
  js: "slash",
  jsx: "slash",
  mjs: "slash",
  javascript: "slash",
  rs: "slash",
  rust: "slash",
  go: "slash",
  java: "slash",
  kt: "slash",
  kotlin: "slash",
  swift: "slash",
  c: "slash",
  h: "slash",
  cpp: "slash",
  cs: "slash",
  css: "slash",
  scss: "slash",
  py: "hash",
  python: "hash",
  sh: "hash",
  bash: "hash",
  zsh: "hash",
  shell: "hash",
  ps1: "hash",
  powershell: "hash",
  rb: "hash",
  ruby: "hash",
  yaml: "hash",
  yml: "hash",
  toml: "hash",
  sql: "sql",
  json: "json",
};

const KEYWORDS: Readonly<Record<Family, ReadonlySet<string>>> = {
  slash: new Set(
    (
      "as async await break case catch class const continue default do else enum export " +
      "extends false fn for func function if impl import in interface let loop match mod " +
      "mut new null package private protected pub public return self static struct super " +
      "switch this throw trait true try type use var void where while yield"
    ).split(" "),
  ),
  hash: new Set(
    (
      "and as def elif else except False finally for from fi if import in is lambda None " +
      "not or pass raise return then True try while with yield do done echo esac export " +
      "local true false null"
    ).split(" "),
  ),
  sql: new Set(
    (
      "select from where insert into update delete create table index join left right " +
      "inner outer on and or not null as order by group having limit values set primary " +
      "key references alter drop distinct union"
    ).split(" "),
  ),
  json: new Set(["true", "false", "null"]),
};

const PATTERNS: Readonly<Record<Family, RegExp>> = {
  slash:
    /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)|("(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*'|`(?:\\.|[^`\\])*`)|(\b\d[\d_.]*\b)|([A-Za-z_]\w*)/g,
  hash: /(#[^\n]*)|("(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*')|(\b\d[\d_.]*\b)|([A-Za-z_]\w*)/g,
  sql: /(--[^\n]*)|('(?:''|[^'])*')|(\b\d[\d.]*\b)|([A-Za-z_]\w*)/g,
  json: /()("(?:\\.|[^"\\])*")|(-?\b\d[\d.eE+-]*\b)|([A-Za-z_]\w*)/g,
};

/** The language a file name or fence label implies, or "" when unknown. */
export function languageOf(nameOrFence: string): string {
  const lower = nameOrFence.toLowerCase();
  const ext = lower.includes(".") ? lower.slice(lower.lastIndexOf(".") + 1) : lower;
  return ext in FAMILY ? ext : "";
}

/** The kind of one match: which capture group hit decides it. */
function classify(
  match: RegExpMatchArray,
  family: Family,
  keywords: ReadonlySet<string>,
): TokenKind {
  const [, comment, string, number, word] = match;
  if (comment !== undefined && comment !== "") {
    return "comment";
  }
  if (string !== undefined) {
    return "string";
  }
  if (number !== undefined) {
    return "number";
  }
  if (word === undefined) {
    return "plain";
  }
  return keywords.has(family === "sql" ? word.toLowerCase() : word) ? "keyword" : "plain";
}

export function highlight(code: string, language: string): readonly Token[] {
  const family = FAMILY[language.toLowerCase()];
  if (family === undefined || code.length > MAX_HIGHLIGHT_CHARS) {
    return [{ kind: "plain", text: code }];
  }
  const keywords = KEYWORDS[family];
  const tokens: Token[] = [];
  let last = 0;
  for (const match of code.matchAll(new RegExp(PATTERNS[family].source, "g"))) {
    if (match.index > last) {
      tokens.push({ kind: "plain", text: code.slice(last, match.index) });
    }
    tokens.push({ kind: classify(match, family, keywords), text: match[0] });
    last = match.index + match[0].length;
  }
  if (last < code.length) {
    tokens.push({ kind: "plain", text: code.slice(last) });
  }
  return tokens;
}
