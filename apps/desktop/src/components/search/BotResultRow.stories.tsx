import type { Story } from "@ladle/react";
import { bot } from "../../test/fixtures";
import BotResultRow from "./BotResultRow";

const noop = (): void => {};

export const WithProject: Story = () => (
  <div style={{ maxWidth: 560 }}>
    <BotResultRow
      bot={bot({ avatar: "icon:orbit" })}
      projectName="Acme"
      active
      onHover={noop}
      onOpen={noop}
    />
    <BotResultRow
      bot={bot({ id: "b2", name: "bob" })}
      projectName="Acme"
      active={false}
      onHover={noop}
      onOpen={noop}
    />
  </div>
);

export const UnknownProject: Story = () => (
  <div style={{ maxWidth: 560 }}>
    <BotResultRow bot={bot()} projectName="" active={false} onHover={noop} onOpen={noop} />
  </div>
);
