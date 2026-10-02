import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { bot } from "../../test/fixtures";
import { botSpy, toastSpy } from "../../test/spies";
import BotRuntimePicker from "./BotRuntimePicker";

function setup(
  daemon = new FakeDaemon(),
  connected = true,
  canControl = true,
): ReturnType<typeof botSpy> {
  const updated = botSpy();
  render(
    <BotRuntimePicker
      client={daemon}
      bot={bot()}
      connected={connected}
      canControl={canControl}
      onBotUpdated={updated}
      onToast={toastSpy()}
    />,
  );
  return updated;
}

describe("BotRuntimePicker", () => {
  it("hides the picker for older daemons", () => {
    setup();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  });
  it("changes runtime only after an explicit save", async () => {
    const user = userEvent.setup();
    const daemon = new FakeDaemon().onRequest("set_bot_runtime", () => ({
      type: "bot",
      req_id: "1",
      bot: bot({ runtime: "codex_cli" }),
    }));
    daemon.capabilities = ["bot_runtime"];
    const updated = setup(daemon);
    expect(screen.getByRole("button", { name: "Change runtime" })).toBeDisabled();
    await user.selectOptions(screen.getByLabelText("Bot runtime"), "codex_cli");
    expect(daemon.requests).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Change runtime" }));
    await waitFor(() => {
      expect(updated).toHaveBeenCalledWith(expect.objectContaining({ runtime: "codex_cli" }));
    });
    expect(daemon.requests[0]?.body).toEqual({
      type: "set_bot_runtime",
      bot_id: "b1",
      runtime: "codex_cli",
    });
  });
  it.each([
    [false, true],
    [true, false],
  ])("disables changes without connection or control (%s, %s)", (connected, control) => {
    const daemon = new FakeDaemon();
    daemon.capabilities = ["bot_runtime"];
    setup(daemon, connected, control);
    expect(screen.getByRole("combobox")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Change runtime" })).toBeDisabled();
  });
  it("reports failed switches and lets the user retry", async () => {
    const user = userEvent.setup();
    const toast = toastSpy();
    const daemon = new FakeDaemon().onRequest("set_bot_runtime", () => {
      throw new Error("CLI unavailable");
    });
    daemon.capabilities = ["bot_runtime"];
    render(
      <BotRuntimePicker
        client={daemon}
        bot={bot()}
        connected
        canControl
        onBotUpdated={botSpy()}
        onToast={toast}
      />,
    );
    await user.selectOptions(screen.getByRole("combobox"), "codex_cli");
    await user.click(screen.getByRole("button", { name: "Change runtime" }));
    await waitFor(() => {
      expect(toast).toHaveBeenCalledWith("error", "Runtime update failed", "CLI unavailable");
    });
    expect(screen.getByRole("button", { name: "Change runtime" })).toBeEnabled();
  });
});
