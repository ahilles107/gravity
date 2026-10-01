import type { Story } from "@ladle/react";
import PermissionCards from "./PermissionCards";

const pending = [
  {
    id: "r1",
    bot_id: "b1",
    tool: "Bash",
    summary: "Bash: rm -rf build && cargo build --release",
    input: '{\n  "command": "rm -rf build && cargo build --release"\n}',
    created_at: "2025-01-15T10:00:00Z",
    expires_at: "2025-01-15T10:10:00Z",
  },
];
const answer = async (): Promise<void> => {};

export const Waiting: Story = () => (
  <div style={{ width: 760 }}>
    <PermissionCards permissions={{ pending, answer }} canAnswer />
  </div>
);

export const ReadOnly: Story = () => (
  <div style={{ width: 760 }}>
    <PermissionCards permissions={{ pending, answer }} canAnswer={false} />
  </div>
);
