import type { Story } from "@ladle/react";
import type { ReactElement, ReactNode } from "react";
import { decision } from "../../test/decisionFixtures";
import PublishTray from "./PublishTray";
import type { NotifyCandidate } from "./publishPlan";

const noop = (): void => {};

function Frame({ children }: { readonly children: ReactNode }): ReactElement {
  return (
    <div className="control-center" style={{ width: 880 }}>
      {children}
    </div>
  );
}

const first = decision({
  id: "d1",
  title: "Waive rule 3 and start the ads today?",
  state: "answered",
  ruling: {
    option: "start",
    text: "Start today, but cap the spend at $40 a day until Friday.",
    answered_at: "2026-09-12T10:00:00Z",
    answered_by: "owner",
  },
});

const second = decision({
  id: "d2",
  title: "Move the Backblaze cap to 4 TB?",
  options: [],
  state: "answered",
  ruling: {
    text: "Yes. Anything over that comes back to me first.",
    answered_at: "2026-09-12T11:00:00Z",
    answered_by: "owner",
  },
});

const candidates: readonly NotifyCandidate[] = [
  {
    botId: "b1",
    name: "auction",
    checked: true,
    locked: true,
    title: "The asking bot is always told",
  },
  { botId: "b2", name: "warden", checked: true, locked: false, title: "Project lead" },
  { botId: "b3", name: "scribe", checked: false, locked: false, title: "" },
];

/** A queue taller than the window: the list scrolls, the actions stay put. */
export const ManyDrafts: Story = () => (
  <div className="control-center" style={{ width: 880, height: 520 }}>
    <PublishTray
      drafts={Array.from({ length: 10 }, (_, index) =>
        decision({
          id: `m${index}`,
          title: `${first.title} (${index + 1})`,
          state: "answered",
          ruling: first.ruling,
        }),
      )}
      candidatesFor={() => candidates}
      chosenFor={() => new Set(["b1", "b2"])}
      onToggle={noop}
      onEdit={noop}
      onPublish={noop}
      onClose={noop}
      busy={false}
      canControl
    />
  </div>
);

export const TwoDrafts: Story = () => (
  <Frame>
    <PublishTray
      drafts={[first, second]}
      candidatesFor={() => candidates}
      chosenFor={() => new Set(["b1", "b2"])}
      onToggle={noop}
      onEdit={noop}
      onPublish={noop}
      onClose={noop}
      busy={false}
      canControl
    />
  </Frame>
);
