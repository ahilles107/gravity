import type { Story } from "@ladle/react";
import type { ReactElement, ReactNode } from "react";
import { decision } from "../../test/decisionFixtures";
import WaitingList from "./WaitingList";

const NOW = Date.parse("2026-09-12T12:00:00Z");
const noop = (): void => {};
const projectName = (): string => "Acme";
const botName = (id: string): string => (id === "b2" ? "warden" : "alice");

function Frame({ children }: { readonly children: ReactNode }): ReactElement {
  return (
    <div className="control-center" style={{ width: 720 }}>
      {children}
    </div>
  );
}

const urgent = decision({
  id: "d1",
  title: "Waive rule 3 and start the ads today?",
  priority: "urgent",
  deadline_at: "2026-09-12T21:00:00Z",
  created_at: "2026-09-12T06:00:00Z",
});

const draft = decision({
  id: "d2",
  title: "Move the Backblaze cap to 4 TB?",
  state: "answered",
  ruling: { text: "Yes, cap at 4 TB.", answered_at: "2026-09-12T10:00:00Z", answered_by: "owner" },
  created_at: "2026-09-11T09:00:00Z",
});

const dated = decision({
  id: "d3",
  title: "Rename the staging bucket before the audit?",
  deadline_at: "2026-09-20T12:00:00Z",
  on_behalf_of_bot_id: "b2",
  created_at: "2026-09-10T09:00:00Z",
});

const undated = decision({
  id: "d4",
  title: "Drop the legacy import path?",
  origin_chain: "b2",
  created_at: "2026-09-08T09:00:00Z",
});

const held = decision({
  id: "d5",
  title: "Sign the year-long CDN contract?",
  state: "held",
  held_until: "2026-09-19T09:00:00Z",
  created_at: "2026-09-05T09:00:00Z",
});

export const Typical: Story = () => (
  <Frame>
    <WaitingList
      waiting={[urgent, draft, dated, undated]}
      held={[held]}
      settledCount={42}
      loaded
      selectedId="d1"
      heldOpen={false}
      onToggleHeld={noop}
      onSelect={noop}
      now={NOW}
      projectName={projectName}
      botName={botName}
    />
  </Frame>
);

export const Empty: Story = () => (
  <Frame>
    <WaitingList
      waiting={[]}
      held={[]}
      settledCount={42}
      loaded
      heldOpen={false}
      onToggleHeld={noop}
      onSelect={noop}
      now={NOW}
      projectName={projectName}
      botName={botName}
    />
  </Frame>
);

export const EmptyNothingSettled: Story = () => (
  <Frame>
    <WaitingList
      waiting={[]}
      held={[]}
      settledCount={0}
      loaded
      heldOpen={false}
      onToggleHeld={noop}
      onSelect={noop}
      now={NOW}
      projectName={projectName}
      botName={botName}
    />
  </Frame>
);
