import type { Story } from "@ladle/react";
import type { ReactElement } from "react";
import type { ChatTurn } from "../../protocol/chat";
import { stats, step, text, turn } from "../../test/chatFixtures";
import { FakeDaemon } from "../../test/fakeDaemon";
import TurnView from "./TurnView";

// A fixed past date: a timestamp from today renders as a bare time, and a
// baseline must not change when the day does.
const AT = "2025-01-15T10:00:00Z";
const client = new FakeDaemon();
const noop = (): void => {};

function Frame({ turns }: { readonly turns: readonly ChatTurn[] }): ReactElement {
  return (
    <div className="chat-scroll" style={{ width: 760 }}>
      {turns.map((t) => (
        <TurnView key={t.id} client={client} turn={t} connected onOpenFile={noop} />
      ))}
    </div>
  );
}

export const OwnerChat: Story = () => (
  <Frame
    turns={[
      turn({
        started_at: AT,
        trigger: { kind: "owner", text: "Which SDK does the Windows build use?", via: "chat" },
        items: [
          text("The build targets the **Windows 11 SDK**. The relevant line:"),
          text(
            '```toml\n[target.x86_64-pc-windows-msvc]\nlinker = "lld-link" # faster links\n```',
            "t2",
          ),
        ],
        stats: stats(),
      }),
    ]}
  />
);

export const TaskWithSteps: Story = () => (
  <Frame
    turns={[
      turn({
        started_at: AT,
        duration_ms: 184_000,
        trigger: {
          kind: "bus",
          from: "lead",
          msg_kind: "task",
          num: 12,
          text: "Port the updater to Windows.",
        },
        items: [
          text("On it — starting with the installer path."),
          step({ id: "s1", title: "Read update.rs", subtitle: "src/update.rs", tool: "Read" }),
          step({
            id: "s2",
            tool: "Edit",
            title: "Edited update.rs",
            subtitle: "src/update.rs",
            added: 18,
            removed: 4,
          }),
          step({
            id: "s3",
            title: "Run tests",
            subtitle: "cargo test -p updater",
            status: "error",
          }),
          {
            type: "sent",
            id: "m1",
            to: "lead",
            msg_kind: "reply",
            body: "Which signing certificate should the MSI use?",
          },
          {
            type: "completed",
            id: "c1",
            task_id: "t-1",
            result: "Ported. The updater builds and its tests pass on Windows.",
            artifacts: [{ path: "/p/artifacts/updater-report.md", name: "updater-report.md" }],
          },
        ],
        stats: stats({
          commands: 1,
          reads: 1,
          edits: 1,
          added: 18,
          removed: 4,
          sent: 1,
          errors: 1,
        }),
      }),
    ]}
  />
);

export const Working: Story = () => (
  <Frame
    turns={[
      turn({
        started_at: AT,
        open: true,
        ended_at: undefined,
        duration_ms: undefined,
        trigger: { kind: "routine", name: "nightly-report", text: "/report" },
        items: [
          step({ id: "s1", title: "Collect yesterday's runs", status: "running" }),
          {
            type: "aside",
            id: "a1",
            kind: "incoming",
            text: "From lead · note: the build is green",
          },
        ],
      }),
    ]}
  />
);
