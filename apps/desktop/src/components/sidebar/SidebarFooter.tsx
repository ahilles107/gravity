import type { ReactElement } from "react";
import { connectionStatusLabel } from "../../protocol/connection";
import type { ConnectionStatus, Endpoint } from "../../protocol/connection";
import type { SettingsCategory } from "../settings/categories";

interface SidebarFooterProps {
  readonly status: ConnectionStatus;
  readonly endpoint: Endpoint;
  readonly canControl: boolean;
  readonly onOpenSettings: (category?: SettingsCategory) => void;
}

export default function SidebarFooter(props: SidebarFooterProps): ReactElement {
  const { status, endpoint, canControl, onOpenSettings } = props;

  return (
    <div className="sidebar-footer">
      <button
        type="button"
        className="row"
        onClick={() => {
          onOpenSettings();
        }}
      >
        <span className="row-glyph">⚙</span>
        <span className="row-name">Settings</span>
      </button>
      <button
        type="button"
        className="conn-line"
        title="Connection settings"
        onClick={() => {
          onOpenSettings("connection");
        }}
      >
        <span className={`conn-dot conn-${status}`} />
        <span className="conn-text">
          {endpoint.host}:{endpoint.port} · {connectionStatusLabel(status)}
        </span>
        {status === "connected" && !canControl ? (
          <span className="readonly-badge" title="This connection lacks the control grant">
            read-only
          </span>
        ) : null}
      </button>
    </div>
  );
}
