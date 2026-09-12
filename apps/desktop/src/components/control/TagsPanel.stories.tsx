import type { Story } from "@ladle/react";
import type { Tag } from "../../protocol/decisions";
import { tag } from "../../test/decisionFixtures";
import TagsPanel from "./TagsPanel";

const ok = (): Promise<boolean> => Promise.resolve(true);

const TAGS: readonly Tag[] = [
  tag({
    id: "t1",
    name: "spend",
    description: "money leaving the account",
    uses: { p1: 3, p2: 2 },
    open_uses: 1,
    last_used_at: "2026-09-10T10:00:00Z",
  }),
  tag({
    id: "t2",
    name: "backups",
    description: "anything that protects data we cannot re-make",
    uses: { p1: 4 },
    open_uses: 0,
    last_used_at: "2026-09-02T10:00:00Z",
  }),
  tag({
    id: "t3",
    name: "infra",
    description: "hosts, images, the network edge",
    uses: { p1: 7 },
    open_uses: 2,
    last_used_at: "2026-08-27T10:00:00Z",
  }),
  tag({ id: "t4", name: "apple-ads", description: "", uses: {}, open_uses: 0 }),
  tag({
    id: "t5",
    name: "retired",
    description: "folded away",
    retired_at: "2026-07-01T00:00:00Z",
  }),
];

export const List: Story = () => (
  <div className="control-center" style={{ width: 960 }}>
    <TagsPanel tags={TAGS} canControl onAdd={ok} onRename={ok} onDescribe={ok} onDelete={ok} />
  </div>
);

export const ReadOnly: Story = () => (
  <div className="control-center" style={{ width: 960 }}>
    <TagsPanel
      tags={TAGS}
      canControl={false}
      onAdd={ok}
      onRename={ok}
      onDescribe={ok}
      onDelete={ok}
    />
  </div>
);
