import type { Story } from "@ladle/react";
import frame from "../../test/browserFrame.jpg?inline";
import { browserActivity, browserTabs } from "../../test/agentFixtures";
import BrowserActivityList from "./BrowserActivityList";
import { BrowserScreen, TabStrip } from "./BrowserPane";

const FRAME = {
  type: "browser_frame",
  bot_id: "b1",
  tab_id: "tab-1",
  data: frame.slice(frame.indexOf(",") + 1),
  width: 640,
  height: 400,
} as const;

export const Live: Story = () => (
  <div className="browser-pane" style={{ width: 1024, height: 640 }}>
    <div className="browser-main">
      <div className="browser-note muted">
        lead uses a browser of its own; your Chrome is off limits.
      </div>
      <TabStrip tabs={browserTabs} onPick={() => undefined} />
      <BrowserScreen tabs={browserTabs} frame={FRAME} name="lead" />
    </div>
    <BrowserActivityList activity={browserActivity} error={null} />
  </div>
);

export const Closed: Story = () => (
  <div className="browser-pane" style={{ width: 1024, height: 400 }}>
    <div className="browser-main">
      <BrowserScreen
        tabs={{ ...browserTabs, open: false, tabs: [], active: null }}
        frame={null}
        name="lead"
      />
    </div>
    <BrowserActivityList activity={[]} error={null} />
  </div>
);
