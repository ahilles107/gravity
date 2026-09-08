import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import * as fx from "../../test/fixtures";
import { botSpy, toastSpy } from "../../test/spies";
import BotHistory from "./BotHistory";

function renderHistory(daemon: FakeDaemon, canControl = true) {
  const onBotUpdated = botSpy();
  const onToast = toastSpy();
  render(
    <BotHistory
      client={daemon}
      bot={fx.bot()}
      canControl={canControl}
      onBotUpdated={onBotUpdated}
      onToast={onToast}
    />,
  );
  return { onBotUpdated, onToast };
}

function withRevisions(revisions: ReturnType<typeof fx.botRevision>[]): FakeDaemon {
  return new FakeDaemon().onRequest("list_bot_revisions", () => ({
    type: "bot_revisions",
    req_id: "1",
    bot_revisions: revisions,
  }));
}

async function open(user: ReturnType<typeof userEvent.setup>): Promise<void> {
  await user.click(screen.getByText("History"));
}

describe("BotHistory", () => {
  it("loads nothing until opened", async () => {
    const daemon = withRevisions([fx.botRevision()]);
    const user = userEvent.setup();
    renderHistory(daemon);
    expect(daemon.requests).toHaveLength(0);

    await open(user);
    await waitFor(() => {
      expect(daemon.requests.some((r) => r.body.type === "list_bot_revisions")).toBe(true);
    });
  });

  it("shows what changed and who changed it", async () => {
    const user = userEvent.setup();
    renderHistory(withRevisions([fx.botRevision({ changed_by: "bot:b1" })]));
    await open(user);

    expect(await screen.findByText("description changed by a bot")).toBeInTheDocument();
    expect(screen.getByText("does things")).toBeInTheDocument();
    expect(screen.getByText("does better things")).toBeInTheDocument();
  });

  it("distinguishes the user's own edits", async () => {
    const user = userEvent.setup();
    renderHistory(withRevisions([fx.botRevision({ changed_by: "user" })]));
    await open(user);
    expect(await screen.findByText("description changed by you")).toBeInTheDocument();
  });

  /** Lifecycle markers are history, not state, so there is nothing to restore. */
  it("offers no revert for creation and deletion markers", async () => {
    const user = userEvent.setup();
    renderHistory(
      withRevisions([
        fx.botRevision({ id: "r1", field: "created", old_value: "", new_value: "created" }),
        fx.botRevision({ id: "r2", field: "deleted", old_value: "alice", new_value: "done" }),
      ]),
    );
    await open(user);

    expect(await screen.findByText("Created by a bot")).toBeInTheDocument();
    expect(screen.getByText("Deleted by a bot")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Revert" })).not.toBeInTheDocument();
  });

  it("reverts a field change and refreshes", async () => {
    const user = userEvent.setup();
    const daemon = withRevisions([fx.botRevision()]).onRequest("revert_bot_revision", () => ({
      type: "bot",
      req_id: "2",
      bot: fx.bot({ description: "does things" }),
    }));
    const { onBotUpdated, onToast } = renderHistory(daemon);
    await open(user);

    await user.click(await screen.findByRole("button", { name: "Revert" }));

    await waitFor(() => {
      expect(onBotUpdated).toHaveBeenCalled();
    });
    const sent = daemon.requests.find((r) => r.body.type === "revert_bot_revision");
    expect(sent?.body).toMatchObject({ revision_id: "r1" });
    expect(onToast).toHaveBeenCalledWith("info", "Reverted", expect.any(String));
  });

  it("surfaces a revert failure", async () => {
    const user = userEvent.setup();
    const daemon = withRevisions([fx.botRevision()]).onRequest("revert_bot_revision", () => {
      throw new Error("gone");
    });
    const { onToast } = renderHistory(daemon);
    await open(user);

    await user.click(await screen.findByRole("button", { name: "Revert" }));
    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("error", "Revert failed", "gone");
    });
  });

  it("says so when there is no history", async () => {
    const user = userEvent.setup();
    renderHistory(withRevisions([]));
    await open(user);
    expect(await screen.findByText("No changes recorded yet.")).toBeInTheDocument();
  });

  it("does not offer revert without the control grant", async () => {
    const user = userEvent.setup();
    renderHistory(withRevisions([fx.botRevision()]), false);
    await open(user);
    expect(await screen.findByRole("button", { name: "Revert" })).toBeDisabled();
  });
});
