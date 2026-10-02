import type { Story } from "@ladle/react";
import { agentsDaemon } from "../../test/agentFixtures";
import * as fx from "../../test/fixtures";
import CommandsPanel from "./CommandsPanel";

export const Overview: Story = () => (
  <div className="bot-info-panel" style={{ width: 360, height: 520 }}>
    <CommandsPanel client={agentsDaemon()} bot={fx.bot()} connected />
  </div>
);
