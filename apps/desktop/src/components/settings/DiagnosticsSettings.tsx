import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { Bot, Delivery, Diagnostics, NotifyLevel } from "../../protocol/entities";
import { errText } from "../../util";
import DeliveriesTable from "../diagnostics/DeliveriesTable";
import DiagnosticsSummary from "../diagnostics/DiagnosticsSummary";
import PanelHeader from "../PanelHeader";

interface DiagnosticsSettingsProps {
  readonly client: DaemonApi;
  readonly bots: readonly Bot[];
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

/** Daemon health summary and the delivery queue, live-updated by pushes. */
export default function DiagnosticsSettings(props: DiagnosticsSettingsProps): ReactElement {
  const { client, bots, connected, canControl, onToast } = props;
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [deliveries, setDeliveries] = useState<readonly Delivery[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (): Promise<void> => {
    try {
      const [diag, list] = await Promise.all([
        client.request({ type: "diagnostics" }, "diagnostics"),
        client.request({ type: "list_deliveries" }, "deliveries"),
      ]);
      setDiagnostics(diag.diagnostics);
      setDeliveries(list.deliveries);
      setError(null);
    } catch (err) {
      setError(errText(err));
    }
  }, [client]);

  useLoadOnConnect(connected, load);

  useEffect(() => {
    return client.on("delivery_update", (push) => {
      setDeliveries((prev) => [push.delivery, ...prev.filter((d) => d.id !== push.delivery.id)]);
    });
  }, [client]);

  const retry = useCallback(
    async (deliveryId: string): Promise<void> => {
      try {
        await client.request({ type: "retry_delivery", delivery_id: deliveryId }, "ok");
        onToast("info", "Delivery retried", "The delivery was re-queued.");
        await load();
      } catch (err) {
        onToast("error", "Retry failed", errText(err));
      }
    },
    [client, load, onToast],
  );

  const botName = useCallback(
    (botId: string): string => bots.find((item) => item.id === botId)?.name ?? botId,
    [bots],
  );

  return (
    <div className="settings-section">
      <div className="panel">
        <PanelHeader title="Daemon health">
          <button
            type="button"
            className="btn btn-small"
            disabled={!connected}
            onClick={() => {
              void load();
            }}
          >
            Refresh
          </button>
        </PanelHeader>
        {error !== null ? <div className="muted">Failed to load: {error}</div> : null}
        {error === null && diagnostics === null ? <div className="muted">Loading…</div> : null}
        {diagnostics === null ? null : (
          <DiagnosticsSummary
            diagnostics={diagnostics}
            serverVersion={client.serverVersion}
            capabilities={client.capabilities}
          />
        )}
      </div>

      <div className="panel">
        <h3 className="panel-title">Deliveries</h3>
        <DeliveriesTable
          deliveries={deliveries}
          botName={botName}
          connected={connected}
          canControl={canControl}
          onRetry={(deliveryId) => {
            void retry(deliveryId);
          }}
        />
      </div>
    </div>
  );
}
