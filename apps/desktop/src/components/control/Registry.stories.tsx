import type { Story } from "@ladle/react";
import { createRef } from "react";
import type { ReactElement, ReactNode } from "react";
import type { Decision, Tag } from "../../protocol/decisions";
import { decision, tag } from "../../test/decisionFixtures";
import { NO_FILTER } from "./decisions";
import Registry from "./Registry";

const noop = (): void => {};
const searchRef = createRef<HTMLInputElement>();

const ruled = (
  id: string,
  at: string,
  over: Partial<Decision>,
  text: string,
  option?: string,
): Decision =>
  decision({
    id,
    state: "settled",
    published_at: at,
    ruling: {
      text,
      answered_at: at,
      answered_by: "owner",
      ...(option === undefined ? {} : { option }),
    },
    ...over,
  });

const ROWS: readonly Decision[] = [
  ruled(
    "d1",
    "2026-09-20T16:00:00Z",
    { title: "Waive rule 3 and start the ads today?", tags: ["spend", "apple-ads"] },
    "Start today. The listing is live and the spend is capped.",
    "start",
  ),
  ruled(
    "d2",
    "2026-09-17T09:30:00Z",
    {
      title: "Drop the nightly Backblaze run?",
      project_id: "p2",
      tags: ["backups"],
      raised_by: { bot_id: "b2", name: "chief", avatar: "color:#7aa2f7" },
      options: [],
    },
    "Keep the nightly. Weekly is not a backup, it is a hope.",
  ),
  ruled(
    "d3",
    "2026-09-11T11:00:00Z",
    {
      title: "Raise the daily ads cap to $90?",
      tags: ["spend"],
      raised_by: { bot_id: "b3", name: "storefront", avatar: "" },
    },
    "Hold at $40 until the listing settles.",
    "hold",
  ),
  decision({
    id: "d4",
    title: "Pin Dozzle to a digest?",
    state: "withdrawn",
    published_at: "2026-09-04T08:00:00Z",
    withdrawn_reason: "the image was archived upstream",
    tags: ["infra"],
    raised_by: { bot_id: "b4", name: "shepherd", avatar: "" },
  }),
  ruled(
    "d5",
    "2026-08-27T15:00:00Z",
    {
      title: "Bump Forgejo to 16?",
      project_id: "p2",
      tags: ["infra"],
      raised_by: { bot_id: "b4", name: "shepherd", avatar: "" },
    },
    "Bump it, but take a snapshot first.",
    "start",
  ),
  ruled(
    "d6",
    "2026-08-12T10:00:00Z",
    { title: "Buy the second Hetzner box this quarter?", tags: ["spend"] },
    "No, not this quarter.",
    "hold",
  ),
];

const TAGS: readonly Tag[] = [
  tag({ id: "t1", name: "spend", uses: { p1: 3 } }),
  tag({ id: "t2", name: "backups", uses: { p1: 1 } }),
  tag({ id: "t3", name: "infra", uses: { p1: 2 } }),
  tag({ id: "t4", name: "apple-ads", uses: { p1: 1 } }),
];

function Frame({ children }: { readonly children: ReactNode }): ReactElement {
  return (
    <div className="control-center" style={{ width: 960 }}>
      {children}
    </div>
  );
}

export const TwoMonths: Story = () => (
  <Frame>
    <Registry
      rows={ROWS}
      settledCount={5}
      tags={TAGS}
      filter={NO_FILTER}
      onQuery={noop}
      onTagFilter={noop}
      onProjectFilter={noop}
      onBotFilter={noop}
      searchRef={searchRef}
      onOpen={noop}
      projectName={(projectId) => (projectId === "p2" ? "Zephyr" : "Acme")}
    />
  </Frame>
);

export const Filtered: Story = () => (
  <Frame>
    <Registry
      rows={ROWS}
      settledCount={5}
      tags={TAGS}
      filter={{ ...NO_FILTER, tagFilter: "spend" }}
      onQuery={noop}
      onTagFilter={noop}
      onProjectFilter={noop}
      onBotFilter={noop}
      searchRef={searchRef}
      onOpen={noop}
      projectName={(projectId) => (projectId === "p2" ? "Zephyr" : "Acme")}
    />
  </Frame>
);
