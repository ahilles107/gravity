import type { ReactElement } from "react";

interface DiffViewProps {
  readonly lines: readonly string[];
}

function lineClass(line: string): string {
  if (line.startsWith("@@")) {
    return "diff-line diff-hunk";
  }
  if (line.startsWith("+")) {
    return "diff-line diff-add";
  }
  if (line.startsWith("-")) {
    return "diff-line diff-del";
  }
  return "diff-line";
}

/** A unified diff, one tinted row per line. */
export default function DiffView({ lines }: DiffViewProps): ReactElement {
  return (
    <pre className="diff-view" aria-label="Changes">
      {lines.map((line, index) => (
        // oxlint-disable-next-line react/no-array-index-key -- diff lines repeat; position is their identity
        <div key={index} className={lineClass(line)}>
          {line === "" ? " " : line}
        </div>
      ))}
    </pre>
  );
}
