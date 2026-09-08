import type { Story } from "@ladle/react";
import NewDeviceForm from "./NewDeviceForm";

export const Default: Story = () => (
  <div style={{ maxWidth: 420 }}>
    <NewDeviceForm onCreate={async (): Promise<void> => {}} onClose={(): void => {}} />
  </div>
);
