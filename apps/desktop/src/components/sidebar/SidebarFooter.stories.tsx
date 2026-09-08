import type { Story } from "@ladle/react";
import SidebarFooter from "./SidebarFooter";

const noop = (): void => {};
const ENDPOINT = { host: "127.0.0.1", port: 4830 } as const;

export const Connected: Story = () => (
  <div style={{ width: 280 }}>
    <SidebarFooter status="connected" endpoint={ENDPOINT} canControl onOpenSettings={noop} />
  </div>
);

export const ReadOnly: Story = () => (
  <div style={{ width: 280 }}>
    <SidebarFooter
      status="connected"
      endpoint={ENDPOINT}
      canControl={false}
      onOpenSettings={noop}
    />
  </div>
);

export const Connecting: Story = () => (
  <div style={{ width: 280 }}>
    <SidebarFooter status="connecting" endpoint={ENDPOINT} canControl onOpenSettings={noop} />
  </div>
);

export const Disconnected: Story = () => (
  <div style={{ width: 280 }}>
    <SidebarFooter status="disconnected" endpoint={ENDPOINT} canControl onOpenSettings={noop} />
  </div>
);

export const AuthFailed: Story = () => (
  <div style={{ width: 280 }}>
    <SidebarFooter status="auth_failed" endpoint={ENDPOINT} canControl onOpenSettings={noop} />
  </div>
);

export const VersionMismatch: Story = () => (
  <div style={{ width: 280 }}>
    <SidebarFooter status="version_mismatch" endpoint={ENDPOINT} canControl onOpenSettings={noop} />
  </div>
);
