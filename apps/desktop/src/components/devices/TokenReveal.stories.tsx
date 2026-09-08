import type { Story } from "@ladle/react";
import TokenReveal from "./TokenReveal";

export const Default: Story = () => (
  <div style={{ maxWidth: 520 }}>
    <TokenReveal
      issued={{ deviceName: "laptop", token: "grav_5f2c9e4ab8d14c6f9a3e7d1b0c8f2a6e" }}
      onDismiss={(): void => {}}
      onToast={(): void => {}}
    />
  </div>
);
