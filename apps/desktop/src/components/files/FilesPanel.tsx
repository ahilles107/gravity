import { useCallback, useState } from "react";
import type { ReactElement } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { Artifact } from "../../protocol/chat";
import type { Bot } from "../../protocol/entities";
import { errText, fmtTimestamp } from "../../util";
import FilePreview from "./FilePreview";
import CreatedBy from "./CreatedBy";

interface FilesPanelProps {
  readonly client: DaemonApi;
  readonly bot: Bot;
  readonly connected: boolean;
  /** The file being previewed, or null for the list. */
  readonly selected: string | null;
  readonly onSelect: (path: string | null) => void;
}

function sizeLabel(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${Math.round(bytes / 1024)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** The project's shared artifacts, newest first, and a preview of one. */
export default function FilesPanel(props: FilesPanelProps): ReactElement {
  const { client, bot, connected, selected, onSelect } = props;
  const [artifacts, setArtifacts] = useState<readonly Artifact[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request(
        { type: "list_artifacts", project_id: bot.project_id },
        "artifacts",
      );
      setArtifacts(reply.artifacts);
      setError(null);
    } catch (failure) {
      setError(errText(failure));
    }
  }, [client, bot.project_id]);

  useLoadOnConnect(connected, load);

  if (selected !== null) {
    return (
      <FilePreview
        client={client}
        botId={bot.id}
        path={selected}
        onBack={() => {
          onSelect(null);
        }}
      />
    );
  }
  return (
    <div className="files-panel">
      <div className="files-head">
        <span className="panel-title">Artifacts</span>
        <button type="button" className="btn btn-small" onClick={() => void load()}>
          Refresh
        </button>
      </div>
      {error === null ? null : <div className="chat-note chat-error">{error}</div>}
      {artifacts.length === 0 && error === null ? (
        <div className="files-empty">No artifacts in this project yet.</div>
      ) : null}
      <ul className="files-list">
        {artifacts.map((artifact) => (
          <li key={artifact.path}>
            <button
              type="button"
              className="files-row"
              title={artifact.rel}
              onClick={() => {
                onSelect(artifact.path);
              }}
            >
              <span className="files-name">{artifact.title ?? artifact.name}</span>
              <span className="files-meta">
                {artifact.title === undefined ? artifact.rel : artifact.name} ·{" "}
                {sizeLabel(artifact.size)}
                {artifact.modified == null ? "" : ` · ${fmtTimestamp(artifact.modified)}`}
              </span>
              {artifact.created_by === undefined ? null : (
                <CreatedBy creator={artifact.created_by} />
              )}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
