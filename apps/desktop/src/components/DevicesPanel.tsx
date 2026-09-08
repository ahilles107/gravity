import { useState } from "react";
import type { ReactElement } from "react";
import type { DaemonApi } from "../protocol/api";
import type { Device, NotifyLevel } from "../protocol/entities";
import DeviceRow from "./devices/DeviceRow";
import NewDeviceForm from "./devices/NewDeviceForm";
import TokenReveal from "./devices/TokenReveal";
import { useDevices } from "./devices/useDevices";
import PanelHeader from "./PanelHeader";
import TableHead from "./TableHead";

interface DevicesPanelProps {
  readonly client: DaemonApi;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

const COLUMNS = ["Name", "Capabilities", "Created", "Last seen", "Status"];

/** Device management: list, create (one-time token reveal), revoke with confirm. */
export default function DevicesPanel(props: DevicesPanelProps): ReactElement {
  const { client, connected, canControl, onToast } = props;
  const devices = useDevices(client, connected, onToast);
  const [creating, setCreating] = useState(false);

  return (
    <div className="panel">
      <PanelHeader title="Devices">
        {canControl ? (
          <button
            type="button"
            className="btn btn-small btn-primary"
            disabled={!connected}
            onClick={() => {
              setCreating((prev) => !prev);
            }}
          >
            + New device
          </button>
        ) : null}
      </PanelHeader>

      {devices.issued === null ? null : (
        <TokenReveal issued={devices.issued} onDismiss={devices.dismissIssued} onToast={onToast} />
      )}

      {creating ? (
        <NewDeviceForm
          onCreate={devices.create}
          onClose={() => {
            setCreating(false);
          }}
        />
      ) : null}

      <DeviceList
        devices={devices.devices}
        loadError={devices.loadError}
        currentDeviceId={client.deviceId}
        connected={connected}
        canControl={canControl}
        onRevoke={(deviceId) => {
          void devices.revoke(deviceId);
        }}
      />
    </div>
  );
}

interface DeviceListProps {
  readonly devices: readonly Device[] | null;
  readonly loadError: string | null;
  readonly currentDeviceId: string | null;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onRevoke: (deviceId: string) => void;
}

function DeviceList(props: DeviceListProps): ReactElement {
  const { devices, loadError, currentDeviceId, connected, canControl, onRevoke } = props;
  if (loadError !== null) {
    return <div className="muted">Failed to load devices: {loadError}</div>;
  }
  if (devices === null) {
    return <div className="muted">Loading…</div>;
  }
  if (devices.length === 0) {
    return <div className="muted">No devices. Create one to connect another client.</div>;
  }
  return (
    <table className="runs-table">
      <TableHead columns={COLUMNS} actionLabel="Actions" />
      <tbody>
        {devices.map((device) => (
          <DeviceRow
            key={device.id}
            device={device}
            isCurrent={currentDeviceId === device.id}
            connected={connected}
            canControl={canControl}
            onRevoke={onRevoke}
          />
        ))}
      </tbody>
    </table>
  );
}
