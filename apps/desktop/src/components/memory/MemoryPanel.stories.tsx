import type { Story } from "@ladle/react";
import { agentsDaemon } from "../../test/agentFixtures";
import * as fx from "../../test/fixtures";
import MemoryPanel from "./MemoryPanel";

export const Facts: Story = () => (
  <div className="bot-info-panel" style={{ width: 360, height: 480 }}>
    <MemoryPanel client={agentsDaemon()} bot={fx.bot()} connected />
  </div>
);
