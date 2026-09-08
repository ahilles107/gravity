import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../../protocol/api";
import { useManagedLocalDaemon } from "../../hooks/useManagedLocalDaemon";
import type { ConnectionStatus, Endpoint } from "../../protocol/connection";
import type { NotifyLevel } from "../../protocol/entities";
import DaemonSettings from "./DaemonSettings";

const STATUS_LABEL: Readonly<Record<ConnectionStatus, string>> = {
  connected: "Connected",
  connecting: "Connecting…",
  disconnected: "Disconnected",
  auth_failed: "Authentication failed",
  version_mismatch: "Daemon version mismatch",
};

interface ConnectionSettingsProps {
  readonly client: DaemonApi;
  readonly status: ConnectionStatus;
  readonly endpoint: Endpoint;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onChangeEndpoint: (endpoint: Endpoint) => void;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

/** Parses the drafted host/port, or null when either is unusable. */
function parseEndpoint(host: string, port: string): Endpoint | null {
  const parsed = Number.parseInt(port, 10);
  if (host.trim().length === 0 || !Number.isInteger(parsed) || parsed <= 0) {
    return null;
  }
  return { host: host.trim(), port: parsed };
}

/** Where the daemon lives, what the connection amounts to, and the daemon's own config. */
export default function ConnectionSettings(props: ConnectionSettingsProps): ReactElement {
  const { client, status, endpoint, connected, canControl, onChangeEndpoint, onToast } = props;
  const [host, setHost] = useState(endpoint.host);
  const [port, setPort] = useState(String(endpoint.port));
  const managedDaemon = useManagedLocalDaemon(endpoint);

  const draft = parseEndpoint(host, port);
  const dirty = draft === null || draft.host !== endpoint.host || draft.port !== endpoint.port;

  return (
    <div className="settings-section">
      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Status</div>
          <div className="settings-row-help">
            {STATUS_LABEL[status]}
            {status === "connected" && !canControl ? " · read-only (no control grant)" : ""}
            {status === "connected" && client.serverVersion.length > 0
              ? ` · daemon ${client.serverVersion}`
              : ""}
          </div>
        </div>
        <span className={`conn-dot conn-${status}`} />
      </div>

      <form
        className="settings-row settings-row-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (draft !== null) {
            onChangeEndpoint(draft);
          }
        }}
      >
        <div className="settings-row-text">
          <div className="settings-row-label">Daemon address</div>
          <div className="settings-row-help">
            The machine running gravityd. Use a Tailscale hostname to attach from another machine.
          </div>
          <div className="settings-inline-fields">
            <input
              aria-label="Daemon host"
              placeholder="Host (e.g. mini.tailnet.ts.net)"
              value={host}
              onChange={(event) => {
                setHost(event.target.value);
              }}
            />
            <input
              aria-label="Daemon port"
              placeholder="Port"
              inputMode="numeric"
              className="settings-port"
              value={port}
              onChange={(event) => {
                setPort(event.target.value);
              }}
            />
            <button
              type="submit"
              className="btn btn-small btn-primary"
              disabled={draft === null || !dirty}
            >
              Connect
            </button>
          </div>
        </div>
      </form>

      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Grants</div>
          <div className="settings-row-help">
            {client.grants.length > 0 ? client.grants.join(", ") : "None (not connected)"}
          </div>
        </div>
      </div>

      <h3 className="settings-subhead">Daemon</h3>
      <DaemonSettings
        client={client}
        connected={connected}
        canControl={canControl}
        canRestart={managedDaemon}
        onToast={onToast}
      />
    </div>
  );
}
