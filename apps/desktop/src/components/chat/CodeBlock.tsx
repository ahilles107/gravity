import type { ReactElement } from "react";
import CopyButton from "../CopyButton";
import { highlight } from "./highlight";

interface CodeBlockProps {
  readonly code: string;
  /** A language or file extension; unknown ones render as plain text. */
  readonly language?: string;
  /** Shown in the header instead of the language, e.g. a file name. */
  readonly label?: string;
}

/** Code with light highlighting, a language label and a copy button. */
export default function CodeBlock({ code, language = "", label }: CodeBlockProps): ReactElement {
  const heading = label ?? language;
  return (
    <div className="code-block">
      <div className="code-block-head">
        <span className="code-block-lang">{heading === "" ? "text" : heading}</span>
        <CopyButton text={code} className="code-block-copy" />
      </div>
      <pre className="code-block-body">
        <code>
          {highlight(code, language).map((token, index) =>
            token.kind === "plain" ? (
              token.text
            ) : (
              // oxlint-disable-next-line react/no-array-index-key -- tokens have no identity beyond position
              <span key={index} className={`tok-${token.kind}`}>
                {token.text}
              </span>
            ),
          )}
        </code>
      </pre>
    </div>
  );
}
