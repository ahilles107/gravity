import { useState } from "react";
import type { ReactElement } from "react";
import type { ProjectRepo } from "../protocol/entities";

interface ProjectRepoFormProps {
  readonly repo: ProjectRepo | null;
  readonly disabled: boolean;
  /** `null` clears the repository. */
  readonly onSave: (repo: ProjectRepo | null) => Promise<void>;
}

/**
 * The project's shared git repository. Workers start from a fresh checkout of
 * its branch and push their work back, which is how they share files —
 * including workers running on a linked machine.
 */
export default function ProjectRepoForm({
  repo,
  disabled,
  onSave,
}: ProjectRepoFormProps): ReactElement {
  const [url, setUrl] = useState(repo?.url ?? "");
  const [branch, setBranch] = useState(repo?.branch ?? "main");
  const [saving, setSaving] = useState(false);

  const next = { url: url.trim(), branch: branch.trim() || "main" };
  const dirty = next.url.length > 0 && (next.url !== repo?.url || next.branch !== repo.branch);

  const run = async (value: ProjectRepo | null): Promise<void> => {
    setSaving(true);
    try {
      await onSave(value);
    } finally {
      setSaving(false);
    }
  };

  return (
    <form
      className="panel"
      onSubmit={(event) => {
        event.preventDefault();
        if (dirty && !disabled && !saving) {
          void run(next);
        }
      }}
    >
      <h3 className="panel-title">Shared repository</h3>
      <div className="field">
        <label className="field-label" htmlFor="project-repo-url">
          Clone URL
        </label>
        <input
          id="project-repo-url"
          type="text"
          placeholder="git@github.com:you/project.git"
          disabled={disabled}
          value={url}
          onChange={(event) => {
            setUrl(event.target.value);
          }}
        />
      </div>
      <div className="field">
        <label className="field-label" htmlFor="project-repo-branch">
          Branch
        </label>
        <input
          id="project-repo-branch"
          type="text"
          disabled={disabled}
          value={branch}
          onChange={(event) => {
            setBranch(event.target.value);
          }}
        />
        <span className="field-hint">
          Each worker a bot spawns clones this branch when it starts and pushes its work back before
          it reports. Whatever a worker leaves unpushed is saved to a branch of its own. Each
          machine uses its own git credentials.
        </span>
      </div>
      <div className="panel-actions">
        {repo !== null ? (
          <button
            type="button"
            className="btn"
            disabled={disabled || saving}
            onClick={() => {
              setUrl("");
              setBranch("main");
              void run(null);
            }}
          >
            Remove
          </button>
        ) : null}
        <button type="submit" className="btn btn-primary" disabled={disabled || saving || !dirty}>
          {saving ? "Saving…" : "Save repository"}
        </button>
      </div>
    </form>
  );
}
