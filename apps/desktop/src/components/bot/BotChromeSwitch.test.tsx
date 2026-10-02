import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Bot, NotifyLevel } from "../../protocol/entities";
import { agentsDaemon } from "../../test/agentFixtures";
import * as fx from "../../test/fixtures";
import BotChromeSwitch from "./BotChromeSwitch";

describe("BotChromeSwitch", () => {
  it("lets the owner allow a bot their Chrome, which restarts it", async () => {
    const client = agentsDaemon().onRequest("set_bot_user_chrome", () => ({
      type: "bot",
      req_id: "1",
      bot: fx.bot({ user_chrome: true }),
    }));
    const onBotUpdated = vi.fn<(bot: Bot) => void>();
    const onToast = vi.fn<(level: NotifyLevel, title: string, body: string) => void>();
    render(
      <BotChromeSwitch
        client={client}
        bot={fx.bot()}
        connected
        canControl
        onBotUpdated={onBotUpdated}
        onToast={onToast}
      />,
    );
    const toggle = screen.getByRole("checkbox", { name: /your Chrome/ });
    expect(toggle).not.toBeChecked();
    await userEvent.click(toggle);
    expect(client.requests.at(-1)?.body).toEqual({
      type: "set_bot_user_chrome",
      bot_id: "b1",
      enabled: true,
    });
    expect(onBotUpdated).toHaveBeenCalledWith(expect.objectContaining({ user_chrome: true }));
    expect(onToast).toHaveBeenCalledWith("info", "Chrome allowed", expect.stringContaining("with"));
  });

  it("reports a refusal", async () => {
    const client = agentsDaemon().onRequest("set_bot_user_chrome", () => {
      throw new Error("bot not found");
    });
    const onToast = vi.fn<(level: NotifyLevel, title: string, body: string) => void>();
    render(
      <BotChromeSwitch
        client={client}
        bot={fx.bot({ user_chrome: true })}
        connected
        canControl
        onBotUpdated={vi.fn<(bot: Bot) => void>()}
        onToast={onToast}
      />,
    );
    await userEvent.click(screen.getByRole("checkbox", { name: /your Chrome/ }));
    expect(onToast).toHaveBeenCalledWith(
      "error",
      "Could not change Chrome access",
      "bot not found",
    );
  });

  it("is absent for a linked bot and on daemons without bot browsers", () => {
    const old = agentsDaemon();
    old.capabilities = ["chat"];
    const { container } = render(
      <>
        <BotChromeSwitch
          client={old}
          bot={fx.bot()}
          connected
          canControl
          onBotUpdated={vi.fn<(bot: Bot) => void>()}
          onToast={vi.fn<(level: NotifyLevel, title: string, body: string) => void>()}
        />
        <BotChromeSwitch
          client={agentsDaemon()}
          bot={fx.bot({ peer: { id: "p", name: "win", online: true } })}
          connected
          canControl
          onBotUpdated={vi.fn<(bot: Bot) => void>()}
          onToast={vi.fn<(level: NotifyLevel, title: string, body: string) => void>()}
        />
      </>,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
