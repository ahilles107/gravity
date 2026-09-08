import type { Story } from "@ladle/react";
import { delivery } from "../../test/fixtures";
import DeliveriesTable from "./DeliveriesTable";

const noop = (): void => {};
const botName = (): string => "alice";

export const AllStates: Story = () => (
  <DeliveriesTable
    deliveries={[
      delivery({ id: "d1", state: "queued" }),
      delivery({ id: "d2", state: "leased", attempt_count: 1 }),
      delivery({ id: "d3", state: "delivered", attempt_count: 1 }),
      delivery({ id: "d4", state: "acknowledged", attempt_count: 1 }),
      delivery({
        id: "d5",
        state: "failed",
        attempt_count: 3,
        next_attempt_at: "2024-05-01T09:30:00Z",
        last_error: "bot session not running",
      }),
    ]}
    botName={botName}
    connected
    canControl
    onRetry={noop}
  />
);

export const Empty: Story = () => (
  <DeliveriesTable deliveries={[]} botName={botName} connected canControl onRetry={noop} />
);

export const ReadOnly: Story = () => (
  <DeliveriesTable
    deliveries={[delivery({ state: "failed", attempt_count: 3, last_error: "timed out" })]}
    botName={botName}
    connected
    canControl={false}
    onRetry={noop}
  />
);
