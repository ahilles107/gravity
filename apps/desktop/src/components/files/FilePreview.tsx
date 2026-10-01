import { useEffect, useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import type { FileBody } from "../../protocol/chat";
import { errText } from "../../util";
import ChatMarkdown from "../chat/ChatMarkdown";
import CodeBlock from "../chat/CodeBlock";
import { languageOf } from "../chat/highlight";
import { dataUrl } from "../chat/useDataUrl";

interface FilePreviewProps {
  readonly client: DaemonApi;
  readonly botId: string;
  readonly path: string;
  readonly onBack: () => void;
}

type PreviewState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly file: FileBody }
  | { readonly status: "failed"; readonly message: string };

/**
 * One file, rendered for reading. Read through the bot's scope, so it reaches
 * the bot's own directory and the project's artifacts and nothing else.
 */
export default function FilePreview({
  client,
  botId,
  path,
  onBack,
}: FilePreviewProps): ReactElement {
  const [state, setState] = useState<PreviewState>({ status: "loading" });

  useEffect(() => {
    let live = true;
    const load = async (): Promise<void> => {
      try {
        const reply = await client.request({ type: "read_file", bot_id: botId, path }, "file");
        if (live) {
          setState({ status: "ready", file: reply.file });
        }
      } catch (failure) {
        if (live) {
          setState({ status: "failed", message: errText(failure) });
        }
      }
    };
    void load();
    return () => {
      live = false;
    };
  }, [client, botId, path]);

  const name = path.split(/[/\\]/).pop() ?? path;
  return (
    <div className="files-panel">
      <div className="files-head">
        <button type="button" className="btn btn-small" onClick={onBack}>
          ← Files
        </button>
        <span className="files-preview-name" title={path}>
          {name}
        </span>
      </div>
      <div className="files-preview">
        {state.status === "loading" ? <div className="files-empty">Loading…</div> : null}
        {state.status === "failed" ? (
          <div className="chat-note chat-error">{state.message}</div>
        ) : null}
        {state.status === "ready" ? <FileBodyView file={state.file} /> : null}
      </div>
    </div>
  );
}

function FileBodyView({ file }: { readonly file: FileBody }): ReactElement {
  const note = file.truncated ? (
    <div className="chat-note">Only the first 16 MB are shown.</div>
  ) : null;
  if (file.mime.startsWith("image/")) {
    const url = dataUrl(file);
    return url === null ? (
      <div className="files-empty">Image unavailable</div>
    ) : (
      <img className="files-image" src={url} alt={file.name} />
    );
  }
  if (file.text === undefined) {
    return <div className="files-empty">No preview for this kind of file ({file.mime}).</div>;
  }
  if (file.mime === "text/markdown") {
    return (
      <>
        {note}
        <ChatMarkdown>{file.text}</ChatMarkdown>
      </>
    );
  }
  return (
    <>
      {note}
      <CodeBlock code={file.text} language={languageOf(file.name)} label={file.name} />
    </>
  );
}
