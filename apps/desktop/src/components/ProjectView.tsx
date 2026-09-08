import { useState } from "react";
import type { ReactElement } from "react";
import type { Bot, Project } from "../protocol/entities";
import ConfirmDialog from "./overlay/ConfirmDialog";

interface ProjectViewProps {
  readonly project: Project;
  readonly bots: readonly Bot[];
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onRename: (projectId: string, name: string) => Promise<void>;
  readonly onDelete: (projectId: string) => Promise<void>;
}

/** How deleting this project is described before it happens. */
export function deletionBody(project: Project, botCount: number): string {
  const bots =
    botCount === 0
      ? "It holds no bots"
      : `Its ${botCount === 1 ? "bot is" : `${botCount} bots are`} stopped and archived`;
  return `Delete “${project.name}”? ${bots} along with their conversations. Workspaces on disk are kept.`;
}

/** Project settings: the name, what it holds, and deletion. */
export default function ProjectView(props: ProjectViewProps): ReactElement {
  const { project, bots, connected, canControl, onRename, onDelete } = props;
  const [name, setName] = useState(project.name);
  const [saving, setSaving] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const trimmed = name.trim();
  const dirty = trimmed.length > 0 && trimmed !== project.name;

  const save = async (): Promise<void> => {
    setSaving(true);
    try {
      await onRename(project.id, trimmed);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="project-view tab-pane-scroll">
      <div className="view-header" data-tauri-drag-region="deep">
        <div className="view-header-main">
          <h2 className="view-title">{project.name}</h2>
          <span className="state-chip">
            {bots.length} {bots.length === 1 ? "bot" : "bots"}
          </span>
        </div>
      </div>

      <div className="panel">
        <h3 className="panel-title">Project</h3>

        <form
          className="field"
          onSubmit={(event) => {
            event.preventDefault();
            if (dirty && connected && canControl && !saving) {
              void save();
            }
          }}
        >
          <label className="field-label" htmlFor="project-name">
            Name
          </label>
          <input
            id="project-name"
            type="text"
            disabled={!canControl}
            value={name}
            onChange={(event) => {
              setName(event.target.value);
            }}
          />
          <span className="field-hint">
            Renaming is cosmetic: the project keeps its folder on disk, so no bot&apos;s workspace
            moves.
          </span>
          <div className="panel-actions">
            <button
              type="submit"
              className="btn btn-primary"
              disabled={!connected || saving || !canControl || !dirty}
            >
              {saving ? "Saving…" : "Save"}
            </button>
          </div>
        </form>

        <dl className="info-meta">
          <dt>Folder</dt>
          <dd className="mono">{project.dir_name}</dd>
          <dt>Created</dt>
          <dd>{project.created_at}</dd>
        </dl>
      </div>

      {canControl ? (
        <div className="panel">
          <h3 className="panel-title">Danger zone</h3>
          <p className="field-hint">{deletionBody(project, bots.length)}</p>
          <div className="panel-actions">
            <button
              type="button"
              className="btn btn-danger"
              disabled={!connected}
              onClick={() => {
                setConfirming(true);
              }}
            >
              Delete project
            </button>
          </div>
        </div>
      ) : null}

      {confirming ? (
        <ConfirmDialog
          title="Delete project"
          body={deletionBody(project, bots.length)}
          confirmLabel="Delete project"
          onConfirm={() => {
            setConfirming(false);
            void onDelete(project.id);
          }}
          onCancel={() => {
            setConfirming(false);
          }}
        />
      ) : null}
    </div>
  );
}
