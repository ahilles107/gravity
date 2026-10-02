import type { ReactElement, ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import { SAFE_LINKS } from "../control/Markdown";
import CodeBlock from "./CodeBlock";

interface ChatMarkdownProps {
  readonly children: string;
}

function text(children: ReactNode): string {
  if (typeof children === "string") {
    return children;
  }
  if (Array.isArray(children)) {
    return children.map((child: ReactNode) => text(child)).join("");
  }
  return "";
}

/**
 * Markdown a bot wrote, for the chat. The link and image policy is the one the
 * Control Center uses (`SAFE_LINKS`): bot output stays untrusted, so links and
 * remote images are inert text. Fenced code becomes a highlighted block.
 */
const COMPONENTS = {
  ...SAFE_LINKS,
  // The block is drawn by `code`; a bare `pre` around it would double the frame.
  pre: ({ children }: { children?: ReactNode }) => <>{children}</>,
  code: ({ className, children }: { className?: string; children?: ReactNode }) => {
    const source = text(children);
    const language = /language-(\S+)/.exec(className ?? "")?.[1];
    if (language === undefined && !source.includes("\n")) {
      return <code className="md-inline-code">{source}</code>;
    }
    return <CodeBlock code={source.replace(/\n$/, "")} language={language ?? ""} />;
  },
};

export default function ChatMarkdown({ children }: ChatMarkdownProps): ReactElement {
  return (
    <div className="markdown chat-markdown">
      <ReactMarkdown components={COMPONENTS}>{children}</ReactMarkdown>
    </div>
  );
}
