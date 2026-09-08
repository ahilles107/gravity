import { useCallback, useState } from "react";
import { capture, captureException } from "../../analytics";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { Device, DeviceCapability, NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";

export interface IssuedToken {
  readonly deviceName: string;
  readonly token: string;
}

export interface DevicesApi {
  /** Null until the first load completes. */
  readonly devices: readonly Device[] | null;
  readonly loadError: string | null;
  readonly issued: IssuedToken | null;
  readonly dismissIssued: () => void;
  readonly create: (name: string, capabilities: readonly DeviceCapability[]) => Promise<void>;
  readonly revoke: (deviceId: string) => Promise<void>;
}

type Toast = (level: NotifyLevel, title: string, body: string) => void;

export function useDevices(client: DaemonApi, connected: boolean, onToast: Toast): DevicesApi {
  const [devices, setDevices] = useState<readonly Device[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [issued, setIssued] = useState<IssuedToken | null>(null);

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request({ type: "list_devices" }, "devices");
      setDevices(reply.devices);
      setLoadError(null);
    } catch (error) {
      captureException(error, "device_list");
      setLoadError(errText(error));
    }
  }, [client]);

  useLoadOnConnect(connected, load);

  const create = useCallback(
    async (name: string, capabilities: readonly DeviceCapability[]): Promise<void> => {
      try {
        const reply = await client.request({ type: "create_device", name, capabilities }, "device");
        capture("device_created", { capabilities });
        setDevices((prev) => [reply.device, ...(prev ?? [])]);
        if (typeof reply.token === "string") {
          setIssued({ deviceName: reply.device.name, token: reply.token });
        } else {
          onToast("warn", "Device created", "The daemon did not return a token.");
        }
      } catch (error) {
        captureException(error, "device_create");
        onToast("error", "Create device failed", errText(error));
      }
    },
    [client, onToast],
  );

  const revoke = useCallback(
    async (deviceId: string): Promise<void> => {
      try {
        const reply = await client.request(
          { type: "revoke_device", device_id: deviceId },
          "device",
        );
        capture("device_revoked", {});
        setDevices((prev) =>
          (prev ?? []).map((device) => (device.id === reply.device.id ? reply.device : device)),
        );
        onToast("info", "Device revoked", `${reply.device.name} can no longer connect.`);
      } catch (error) {
        captureException(error, "device_revoke");
        onToast("error", "Revoke failed", errText(error));
      }
    },
    [client, onToast],
  );

  const dismissIssued = useCallback((): void => {
    setIssued(null);
  }, []);

  return { devices, loadError, issued, dismissIssued, create, revoke };
}
