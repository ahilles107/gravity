import type { Story } from "@ladle/react";
import type { ReactElement, ReactNode } from "react";
import { bot, botActivity } from "../../test/fixtures";
import BotRow from "./BotRow";

const noop = (): void => {};

function SidebarFrame({ children }: { readonly children: ReactNode }): ReactElement {
  return <div style={{ width: 280 }}>{children}</div>;
}

export const States: Story = () => (
  <SidebarFrame>
    {(["ready", "working", "waiting_for_user", "rate_limited", "crashed", "stopped"] as const).map(
      (state) => (
        <BotRow
          key={state}
          bot={bot({ id: state, name: state.replaceAll("_", " "), state })}
          unread={0}
          failed={0}
          next={undefined}
          activity={botActivity({ bot_id: state })}
          selected={false}
          canControl
          onClick={noop}
          onDelete={noop}
          onTogglePin={noop}
        />
      ),
    )}
  </SidebarFrame>
);

export const Selected: Story = () => (
  <SidebarFrame>
    <BotRow
      bot={bot()}
      unread={0}
      failed={0}
      next={undefined}
      activity={botActivity()}
      selected
      canControl
      onClick={noop}
      onDelete={noop}
      onTogglePin={noop}
    />
  </SidebarFrame>
);

export const WithBadges: Story = () => (
  <SidebarFrame>
    <BotRow
      bot={bot()}
      unread={3}
      failed={2}
      next="2024-05-01T12:30:00.000Z"
      activity={botActivity({ from: "bob", text: "report is ready for review" })}
      selected={false}
      canControl
      onClick={noop}
      onDelete={noop}
      onTogglePin={noop}
    />
  </SidebarFrame>
);

export const SilentBot: Story = () => (
  <SidebarFrame>
    <BotRow
      bot={bot({ description: "keeps the changelog tidy" })}
      unread={0}
      failed={0}
      next={undefined}
      activity={undefined}
      selected={false}
      canControl
      onClick={noop}
      onDelete={noop}
      onTogglePin={noop}
    />
  </SidebarFrame>
);
