import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FakeDaemon } from "../test/fakeDaemon";
import * as fx from "../test/fixtures";
import { botSpy, toastSpy } from "../test/spies";
import InfoPanel from "./InfoPanel";

const revealBotWorkspace = vi.hoisted(() => vi.fn<(workspacePath: string) => Promise<void>>());
vi.mock("../reveal", () => ({ revealBotWorkspace }));

function renderPanel(
  daemon: FakeDaemon,
  over: { canControl?: boolean; connected?: boolean; bot?: ReturnType<typeof fx.bot> } = {},
) {
  const onBotUpdated = botSpy();
  const onToast = toastSpy();
  const connected = over.connected ?? true;
  const canControl = over.canControl ?? true;
  const bot = over.bot ?? fx.bot({ state_reason: "idle" });
  const view = render(
    <InfoPanel
      client={daemon}
      bot={bot}
      connected={connected}
      canControl={canControl}
      onBotUpdated={onBotUpdated}
      onToast={onToast}
    />,
  );
  const rerenderBot = (nextBot: ReturnType<typeof fx.bot>): void => {
    view.rerender(
      <InfoPanel
        client={daemon}
        bot={nextBot}
        connected={connected}
        canControl={canControl}
        onBotUpdated={onBotUpdated}
        onToast={onToast}
      />,
    );
  };
  return { onBotUpdated, onToast, rerenderBot };
}

function field(label: string): HTMLElement {
  return screen.getByLabelText(label, { exact: false });
}

describe("InfoPanel", () => {
  beforeEach(() => {
    revealBotWorkspace.mockReset();
    revealBotWorkspace.mockResolvedValue();
  });

  it("shows the bot metadata", () => {
    renderPanel(new FakeDaemon());
    expect(screen.getByText("/tmp/alice")).toBeInTheDocument();
    expect(screen.getByText("ready — idle")).toBeInTheDocument();
  });

  it("reveals the bot workspace in Finder", async () => {
    const user = userEvent.setup();
    renderPanel(new FakeDaemon());

    await user.click(screen.getByRole("button", { name: "Reveal bot workspace in Finder" }));

    expect(revealBotWorkspace).toHaveBeenCalledWith("/tmp/alice");
  });

  it("omits the reason when there is none", () => {
    renderPanel(new FakeDaemon(), { bot: fx.bot({ state_reason: "" }) });
    expect(screen.getByText("ready")).toBeInTheDocument();
  });

  it("shows the bot's current instructions rather than an empty box", () => {
    renderPanel(new FakeDaemon(), { bot: fx.bot({ instructions: "be terse" }) });
    expect(screen.getByDisplayValue("be terse")).toBeInTheDocument();
  });

  it("notes when a bot was created by another bot", () => {
    renderPanel(new FakeDaemon(), { bot: fx.bot({ created_by_bot_id: "b9" }) });
    expect(screen.getByText("Created by another bot")).toBeInTheDocument();
  });

  /** A no-op save would record a revision the user never made. */
  it("cannot be saved until something changes", async () => {
    const user = userEvent.setup();
    renderPanel(new FakeDaemon());
    const save = screen.getByRole("button", { name: "Save" });
    expect(save).toBeDisabled();

    await user.type(field("Description"), "!");
    expect(save).toBeEnabled();
  });

  it("sends only the fields that changed", async () => {
    const user = userEvent.setup();
    const daemon = new FakeDaemon().onRequest("update_bot", () => ({
      type: "bot",
      req_id: "1",
      bot: fx.bot({ description: "does things updated" }),
    }));
    const { onBotUpdated, onToast } = renderPanel(daemon);

    await user.type(field("Description"), " updated");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(onBotUpdated).toHaveBeenCalled();
    });
    const sent = daemon.requests.find((r) => r.body.type === "update_bot");
    expect(sent?.body).toEqual({
      type: "update_bot",
      bot_id: "b1",
      description: "does things updated",
    });
    expect(onToast).toHaveBeenCalledWith("info", "Bot updated", expect.any(String));
  });

  it("can rename and re-avatar in one save", async () => {
    const user = userEvent.setup();
    const daemon = new FakeDaemon().onRequest("update_bot", () => ({
      type: "bot",
      req_id: "1",
      bot: fx.bot({ name: "alice2", avatar: "icon:nova" }),
    }));
    renderPanel(daemon);

    await user.type(field("Name"), "2");
    await user.click(screen.getByRole("radio", { name: "nova" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(daemon.requests.some((r) => r.body.type === "update_bot")).toBe(true);
    });
    const sent = daemon.requests.find((r) => r.body.type === "update_bot");
    expect(sent?.body).toMatchObject({ name: "alice2", avatar: "icon:nova" });
  });

  /** The picker is the only avatar control, so it must show what is set. */
  it("marks the bot's current icon as chosen", () => {
    renderPanel(new FakeDaemon(), { bot: fx.bot({ avatar: "icon:tide" }) });
    expect(screen.getByRole("radio", { name: "tide" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "nova" })).not.toBeChecked();
  });

  it("reflects a bot-driven rename and avatar change", () => {
    const { rerenderBot } = renderPanel(new FakeDaemon());

    rerenderBot(fx.bot({ name: "Thrasymachus", avatar: "icon:rune" }));

    expect(field("Name")).toHaveValue("Thrasymachus");
    expect(screen.getByRole("radio", { name: "rune" })).toBeChecked();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("preserves edited fields while accepting other bot-driven changes", async () => {
    const user = userEvent.setup();
    const { rerenderBot } = renderPanel(new FakeDaemon());
    await user.clear(field("Name"));
    await user.type(field("Name"), "Local draft");

    rerenderBot(fx.bot({ name: "Thrasymachus", avatar: "icon:rune" }));

    expect(field("Name")).toHaveValue("Local draft");
    expect(screen.getByRole("radio", { name: "rune" })).toBeChecked();
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
  });

  it("surfaces a save failure", async () => {
    const user = userEvent.setup();
    const daemon = new FakeDaemon().onRequest("update_bot", () => {
      throw new Error("read-only");
    });
    const { onToast } = renderPanel(daemon);

    await user.type(field("Description"), "!");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("error", "Update failed", "read-only");
    });
  });

  it("disables editing without the control grant", () => {
    renderPanel(new FakeDaemon(), { canControl: false });
    for (const box of screen.getAllByRole("textbox")) {
      expect(box).toBeDisabled();
    }
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });
});
