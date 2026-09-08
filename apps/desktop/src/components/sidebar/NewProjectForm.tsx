import { useState } from "react";
import type { ReactElement } from "react";

interface NewProjectFormProps {
  /** Result ignored: the form only kicks the creation off. */
  readonly onCreate: (name: string) => Promise<unknown>;
  /** Omitted where the form has nothing to close into, e.g. first run. */
  readonly onClose?: () => void;
}

export default function NewProjectForm({ onCreate, onClose }: NewProjectFormProps): ReactElement {
  const [name, setName] = useState("");
  return (
    <form
      className="mini-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (name.trim().length === 0) {
          return;
        }
        void onCreate(name.trim());
        onClose?.();
      }}
    >
      <input
        autoFocus
        placeholder="Project name"
        value={name}
        onChange={(event) => {
          setName(event.target.value);
        }}
      />
      <div className="mini-form-actions">
        <button type="submit" className="btn btn-small btn-primary">
          Create
        </button>
        {onClose === undefined ? null : (
          <button type="button" className="btn btn-small" onClick={onClose}>
            Cancel
          </button>
        )}
      </div>
    </form>
  );
}
