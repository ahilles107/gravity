import type { ReactElement } from "react";
import type { Delivery } from "../../protocol/entities";
import { fmtTimestamp } from "../../util";
import TableHead from "../TableHead";

const COLUMNS = ["Bot", "State", "Attempts", "Next attempt", "Last error", "Created"];

interface DeliveriesTableProps {
  readonly deliveries: readonly Delivery[];
  readonly botName: (botId: string) => string;
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly onRetry: (deliveryId: string) => void;
}

export default function DeliveriesTable(props: DeliveriesTableProps): ReactElement {
  const { deliveries, botName, connected, canControl, onRetry } = props;
  if (deliveries.length === 0) {
    return <div className="muted">No deliveries.</div>;
  }
  return (
    <table className="runs-table">
      <TableHead columns={COLUMNS} actionLabel="Actions" />
      <tbody>
        {deliveries.map((delivery) => (
          <tr key={delivery.id}>
            <td>{botName(delivery.bot_id)}</td>
            <td>
              <span className={`run-state delivery-${delivery.state}`}>{delivery.state}</span>
            </td>
            <td>{delivery.attempt_count}</td>
            <td>
              {typeof delivery.next_attempt_at === "string"
                ? fmtTimestamp(delivery.next_attempt_at)
                : "–"}
            </td>
            <td className="run-error">
              {typeof delivery.last_error === "string" ? delivery.last_error : ""}
            </td>
            <td>{fmtTimestamp(delivery.created_at)}</td>
            <td>
              {delivery.state === "failed" && canControl ? (
                <button
                  type="button"
                  className="btn btn-small"
                  disabled={!connected}
                  onClick={() => {
                    onRetry(delivery.id);
                  }}
                >
                  Retry
                </button>
              ) : null}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
