import { useState } from "react";
import type { ReactElement } from "react";
import type { DeviceCapability } from "../../protocol/entities";

interface NewDeviceFormProps {
  readonly onCreate: (name: string, capabilities: readonly DeviceCapability[]) => Promise<void>;
  readonly onClose: () => void;
}

export default function NewDeviceForm({ onCreate, onClose }: NewDeviceFormProps): ReactElement {
  const [name, setName] = useState("");
  const [capabilities, setCapabilities] = useState<readonly DeviceCapability[]>(["read"]);
  const [submitting, setSubmitting] = useState(false);

  const toggle = (capability: DeviceCapability, on: boolean): void => {
    setCapabilities((prev) =>
      on ? [...prev, capability] : prev.filter((item) => item !== capability),
    );
  };

  const valid = name.trim().length > 0 && capabilities.length > 0;

  const submit = async (): Promise<void> => {
    setSubmitting(true);
    try {
      await onCreate(name.trim(), capabilities);
      onClose();
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form
      className="routine-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (valid) {
          void submit();
        }
      }}
    >
      <label className="field">
        <span className="field-label">Device name</span>
        <input
          autoFocus
          placeholder="phone, laptop, ci-runner…"
          value={name}
          onChange={(event) => {
            setName(event.target.value);
          }}
        />
      </label>
      <div className="field">
        <span className="field-label">Capabilities</span>
        <div className="cap-checks">
          {(["read", "control"] as const).map((capability) => (
            <label key={capability} className="member-option">
              <input
                type="checkbox"
                checked={capabilities.includes(capability)}
                onChange={(event) => {
                  toggle(capability, event.target.checked);
                }}
              />
              {capability}
            </label>
          ))}
        </div>
      </div>
      <div className="panel-actions">
        <button type="submit" className="btn btn-primary" disabled={submitting || !valid}>
          {submitting ? "Creating…" : "Create device"}
        </button>
        <button type="button" className="btn" onClick={onClose}>
          Cancel
        </button>
      </div>
    </form>
  );
}
