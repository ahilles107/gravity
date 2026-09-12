import type { ReactElement, ReactNode } from "react";
import ReactMarkdown from "react-markdown";

interface MarkdownProps {
  readonly children: string;
}

/**
 * Links are text, not anchors. A bot could otherwise put a `javascript:` URL or
 * a plausible-looking phishing target behind trustworthy words, and nothing in
 * a decision body needs to be clickable.
 *
 * Images are text for the same reason plus one more: a remote `src` is fetched
 * on render, so it is a read receipt telling whoever hosts it exactly when the
 * owner opened the decision. Nothing stops it downstream — `tauri.conf.json`
 * sets no CSP.
 */
const COMPONENTS = {
  a: ({ children, href }: { children?: ReactNode; href?: string }) => (
    <span className="markdown-link" title={href}>
      {children}
    </span>
  ),
  img: ({ alt, src }: { alt?: string; src?: string }) => (
    <span className="markdown-link" title={src}>
      {alt === undefined || alt === "" ? "image" : alt}
    </span>
  ),
};

/**
 * Markdown written by a bot.
 *
 * Every decision body is model output, so this is untrusted input rendered
 * into the owner's window. The safety here is that `react-markdown` does not
 * pass raw HTML through unless a plugin asks it to, and no plugin does: this
 * component is the single place that policy lives, so adding `rehype-raw`
 * anywhere would be a visible change rather than a quiet one.
 *
 */
export default function Markdown({ children }: MarkdownProps): ReactElement {
  return (
    <div className="markdown">
      <ReactMarkdown components={COMPONENTS}>{children}</ReactMarkdown>
    </div>
  );
}
