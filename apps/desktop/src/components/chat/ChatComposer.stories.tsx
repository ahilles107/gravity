import type { Story } from "@ladle/react";
import ChatComposer from "./ChatComposer";

const send = async (): Promise<void> => {};

export const Ready: Story = () => (
  <div style={{ width: 640 }}>
    <ChatComposer disabledReason={null} placeholder="Message windev" onSend={send} />
  </div>
);

export const ReadOnly: Story = () => (
  <div style={{ width: 640 }}>
    <ChatComposer
      disabledReason="Read-only connection"
      placeholder="Message windev"
      onSend={send}
    />
  </div>
);
