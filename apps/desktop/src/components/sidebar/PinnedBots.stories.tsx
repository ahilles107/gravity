import type { Story } from "@ladle/react";
import { bot } from "../../test/fixtures";
import PinnedBots from "./PinnedBots";

const noop = (): void => {};

const BOTS = [
  bot({ id: "b1", name: "alice", avatar: "icon:orbit", state: "working" }),
  bot({ id: "b2", name: "bob", avatar: "icon:ember", state: "ready" }),
  bot({ id: "b3", name: "carol", avatar: "", state: "crashed" }),
] as const;

export const Strip: Story = () => (
  <div style={{ width: 280 }}>
    <PinnedBots
      bots={BOTS}
      unreadBots={{ b2: 4 }}
      selectedBotId="b1"
      canControl
      onSelect={noop}
      onDelete={noop}
      onTogglePin={noop}
    />
  </div>
);

export const SinglePin: Story = () => (
  <div style={{ width: 280 }}>
    <PinnedBots
      bots={[bot({ name: "alice", avatar: "icon:nova" })]}
      unreadBots={{}}
      selectedBotId={null}
      canControl
      onSelect={noop}
      onDelete={noop}
      onTogglePin={noop}
    />
  </div>
);
