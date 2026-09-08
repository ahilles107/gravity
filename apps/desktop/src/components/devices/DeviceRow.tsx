import { useState } from "react";
import type { ReactElement } from "react";
import type { Device } from "../../protocol/entities";
import { fmtTimestamp } from "../../util";

interface DeviceRowProps {
  readonly device: Device;
  readonly isCurrent: boolean;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onRevoke: (deviceId: string) => void;
}

function isRevoked(device: Device): boolean {
  return typeof device.revoked_at === "string" && device.revoked_at.length > 0;
}

export default function DeviceRow({
  device,
  isCurrent,
  connected,
  canControl,
  onRevoke,
}: DeviceRowProps): ReactElement {
  const [confirming, setConfirming] = useState(false);
  const revoked = isRevoked(device);

  return (
    <tr className={revoked ? "device-revoked" : ""}>
      <td>
        <span className="device-name">{device.name}</span>
        {isCurrent ? <span className="muted"> (this device)</span> : null}
      </td>
      <td>{device.capabilities.join(", ")}</td>
      <td>{fmtTimestamp(device.created_at)}</td>
      <td>
        {typeof device.last_seen_at === "string" ? fmtTimestamp(device.last_seen_at) : "never"}
      </td>
      <td>
        <span className={`run-state delivery-${revoked ? "failed" : "acknowledged"}`}>
          {revoked ? "revoked" : "active"}
        </span>
      </td>
      <td>
        {canControl && !revoked ? (
          <RevokeControl
            confirming={confirming}
            connected={connected}
            onAsk={() => {
              setConfirming(true);
            }}
            onCancel={() => {
              setConfirming(false);
            }}
            onConfirm={() => {
              setConfirming(false);
              onRevoke(device.id);
            }}
          />
        ) : null}
      </td>
    </tr>
  );
}

interface RevokeControlProps {
  readonly confirming: boolean;
  readonly connected: boolean;
  readonly onAsk: () => void;
  readonly onCancel: () => void;
  readonly onConfirm: () => void;
}

function RevokeControl(props: RevokeControlProps): ReactElement {
  const { confirming, connected, onAsk, onCancel, onConfirm } = props;
  if (!confirming) {
    return (
      <button type="button" className="btn btn-small" disabled={!connected} onClick={onAsk}>
        Revoke
      </button>
    );
  }
  return (
    <span className="revoke-confirm">
      <button
        type="button"
        className="btn btn-small btn-danger"
        disabled={!connected}
        onClick={onConfirm}
      >
        Confirm revoke
      </button>
      <button type="button" className="btn btn-small" onClick={onCancel}>
        Cancel
      </button>
    </span>
  );
}
