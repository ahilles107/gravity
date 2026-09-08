import type { Story } from "@ladle/react";
import type { ReactElement, ReactNode } from "react";
import TableHead from "../TableHead";
import { device } from "../../test/fixtures";
import DeviceRow from "./DeviceRow";

const noop = (): void => {};

function DevicesTable({ children }: { readonly children: ReactNode }): ReactElement {
  return (
    <table className="runs-table">
      <TableHead
        columns={["Name", "Capabilities", "Created", "Last seen", "State"]}
        actionLabel="Actions"
      />
      <tbody>{children}</tbody>
    </table>
  );
}

export const Active: Story = () => (
  <DevicesTable>
    <DeviceRow
      device={device({ capabilities: ["read", "control"], last_seen_at: "2024-05-01T10:00:00Z" })}
      isCurrent={false}
      connected
      canControl
      onRevoke={noop}
    />
  </DevicesTable>
);

export const CurrentDevice: Story = () => (
  <DevicesTable>
    <DeviceRow device={device()} isCurrent connected canControl onRevoke={noop} />
  </DevicesTable>
);

export const Revoked: Story = () => (
  <DevicesTable>
    <DeviceRow
      device={device({ name: "old-phone", revoked_at: "2024-05-01T10:00:00Z" })}
      isCurrent={false}
      connected
      canControl
      onRevoke={noop}
    />
  </DevicesTable>
);
