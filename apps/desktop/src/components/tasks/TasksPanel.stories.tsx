import type { Story } from "@ladle/react";
import * as fx from "../../test/fixtures";
import { tasksDaemon } from "../../test/taskFixtures";
import TasksPanel from "./TasksPanel";

const client = tasksDaemon();

export const Overview: Story = () => (
  <div className="bot-info-panel" style={{ width: 360, height: 640 }}>
    <TasksPanel client={client} bot={fx.bot()} connected />
  </div>
);
