import type { Story } from "@ladle/react";
import { agentsDaemon, lead, projectBots, qa, thread, windev } from "../../test/agentFixtures";
import * as fx from "../../test/fixtures";
import { ThreadMessages } from "./ConversationThread";
import ConversationsView from "./ConversationsView";

const bots = new Map([lead, windev, qa].map((bot) => [bot.id, bot]));

export const TwoBots: Story = () => (
  <div style={{ width: 760, padding: 18 }}>
    <ThreadMessages messages={thread} bots={bots} left={lead.id} />
  </div>
);

export const WithPairs: Story = () => (
  <div style={{ width: 1024, height: 640, display: "flex" }}>
    <ConversationsView
      client={agentsDaemon()}
      project={fx.project()}
      bots={projectBots}
      connected
    />
  </div>
);
