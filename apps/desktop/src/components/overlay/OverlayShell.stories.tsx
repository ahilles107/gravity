import type { Story } from "@ladle/react";
import OverlayShell from "./OverlayShell";

const noop = (): void => {};

export const WithContent: Story = () => (
  <OverlayShell label="Example overlay" onClose={noop}>
    <div style={{ padding: 16 }}>
      <p>Any centred panel content goes here.</p>
    </div>
  </OverlayShell>
);
