import { useState } from "react";
import type { ReactElement } from "react";

interface CopyButtonProps {
  readonly text: string;
  /** What is copied, for screen readers: "Copy command". */
  readonly label?: string;
  readonly className?: string;
}

/** Copies `text` to the clipboard, saying "Copied" for a moment. */
export default function CopyButton({
  text,
  label = "Copy",
  className = "copy-button",
}: CopyButtonProps): ReactElement {
  const [copied, setCopied] = useState(false);
  const copy = async (): Promise<void> => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => {
        setCopied(false);
      }, 1500);
    } catch {
      setCopied(false);
    }
  };
  return (
    <button type="button" className={className} aria-label={label} onClick={() => void copy()}>
      {copied ? "Copied" : "Copy"}
    </button>
  );
}
