import type { Story } from "@ladle/react";
import type { ReactElement, ReactNode } from "react";
import { pendingCounts } from "../../test/decisionFixtures";
import ControlCenterRow from "./ControlCenterRow";

const noop = (): void => {};

function SidebarFrame({ children }: { readonly children: ReactNode }): ReactElement {
  return <div style={{ width: 280 }}>{children}</div>;
}

export const Quiet: Story = () => (
  <SidebarFrame>
    <ControlCenterRow counts={pendingCounts({ total: 3 })} selected={false} onSelect={noop} />
  </SidebarFrame>
);

export const Urgent: Story = () => (
  <SidebarFrame>
    <ControlCenterRow
      counts={pendingCounts({ total: 3, urgent: 1, due_soon: 2 })}
      selected={false}
      onSelect={noop}
    />
  </SidebarFrame>
);

export const Selected: Story = () => (
  <SidebarFrame>
    <ControlCenterRow counts={pendingCounts({ total: 3 })} selected onSelect={noop} />
  </SidebarFrame>
);
