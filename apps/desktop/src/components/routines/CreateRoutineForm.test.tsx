import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import * as fx from "../../test/fixtures";
import type { OverlapPolicy, RoutineTrigger } from "../../protocol/entities";
import CreateRoutineForm from "./CreateRoutineForm";
import type { RoutineLimits } from "./useRoutines";

type Submit = (
  name: string,
  trigger: RoutineTrigger,
  prompt: string,
  overlapPolicy: OverlapPolicy,
  limits: RoutineLimits,
) => Promise<boolean>;

const NO_LIMITS = { maxDurationSeconds: undefined, maxAttempts: undefined };

function renderForm() {
  const onSubmit = vi.fn<Submit>(() => Promise.resolve(true));
  const onClose = vi.fn<() => void>();
  render(
    <CreateRoutineForm
      bots={[fx.bot({ id: "b1", name: "alice" }), fx.bot({ id: "b2", name: "bob" })]}
      onSubmit={onSubmit}
      onClose={onClose}
    />,
  );
  return { onSubmit, onClose };
}

async function fillNameAndPrompt(user: ReturnType<typeof userEvent.setup>): Promise<void> {
  await user.type(screen.getByPlaceholderText("weekly-report"), "nightly");
  await user.type(screen.getByPlaceholderText("/weekly-report"), "/nightly");
}

describe("CreateRoutineForm", () => {
  it("submits a cron trigger by default", async () => {
    const user = userEvent.setup();
    const { onSubmit, onClose } = renderForm();

    await fillNameAndPrompt(user);
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    expect(onSubmit).toHaveBeenCalledWith(
      "nightly",
      { kind: "cron", expr: "0 0 9 * * MON", tz: "Europe/Warsaw" },
      "/nightly",
      "skip",
      NO_LIMITS,
    );
    expect(onClose).toHaveBeenCalled();
  });

  it("edits the cron expression and timezone", async () => {
    const user = userEvent.setup();
    const { onSubmit } = renderForm();

    await fillNameAndPrompt(user);
    const tz = screen.getByDisplayValue("Europe/Warsaw");
    await user.clear(tz);
    await user.type(tz, "UTC");
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    expect(onSubmit).toHaveBeenCalledWith(
      "nightly",
      expect.objectContaining({ tz: "UTC" }),
      "/nightly",
      "skip",
      NO_LIMITS,
    );
  });

  it("switches to an interval trigger", async () => {
    const user = userEvent.setup();
    const { onSubmit } = renderForm();

    await fillNameAndPrompt(user);
    await user.selectOptions(screen.getByLabelText(/Trigger/), "interval");
    const seconds = screen.getByDisplayValue("3600");
    await user.clear(seconds);
    await user.type(seconds, "900");
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    expect(onSubmit).toHaveBeenCalledWith(
      "nightly",
      { kind: "interval", seconds: 900 },
      "/nightly",
      "skip",
      NO_LIMITS,
    );
  });

  it("switches to a signal trigger and narrows it to one bot", async () => {
    const user = userEvent.setup();
    const { onSubmit } = renderForm();

    await fillNameAndPrompt(user);
    await user.selectOptions(screen.getByLabelText(/Trigger/), "signal");
    await user.type(screen.getByPlaceholderText("deploy.finished"), "deploy.finished");
    await user.selectOptions(screen.getByLabelText(/From bot/), "b2");
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    expect(onSubmit).toHaveBeenCalledWith(
      "nightly",
      { kind: "signal", name: "deploy.finished", from_bot_id: "b2" },
      "/nightly",
      "skip",
      NO_LIMITS,
    );
  });

  it("passes a deadline and an attempt budget when they are set", async () => {
    const user = userEvent.setup();
    const { onSubmit } = renderForm();

    await fillNameAndPrompt(user);
    await user.type(screen.getByPlaceholderText("default"), "120");
    await user.type(screen.getByPlaceholderText("1"), "3");
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    expect(onSubmit).toHaveBeenCalledWith("nightly", expect.anything(), "/nightly", "skip", {
      maxDurationSeconds: 120,
      maxAttempts: 3,
    });
  });

  it("shows bounds errors instead of silently dropping invalid limits", async () => {
    const user = userEvent.setup();
    const { onSubmit } = renderForm();

    await fillNameAndPrompt(user);
    await user.type(screen.getByPlaceholderText("default"), "21601");
    await user.type(screen.getByPlaceholderText("1"), "6");
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    expect(screen.getByText("Duration must be between 1 and 21600.")).toBeInTheDocument();
    expect(screen.getByText("Attempts must be between 1 and 5.")).toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("keeps the form open when creation fails", async () => {
    const user = userEvent.setup();
    const { onSubmit, onClose } = renderForm();
    onSubmit.mockResolvedValue(false);

    await fillNameAndPrompt(user);
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    expect(onClose).not.toHaveBeenCalled();
  });

  it("changes the overlap policy", async () => {
    const user = userEvent.setup();
    const { onSubmit } = renderForm();

    await fillNameAndPrompt(user);
    await user.selectOptions(screen.getByLabelText(/Overlap policy/), "replace");
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    expect(onSubmit).toHaveBeenCalledWith(
      "nightly",
      expect.anything(),
      "/nightly",
      "replace",
      NO_LIMITS,
    );
  });

  it("refuses to submit an incomplete trigger", async () => {
    const user = userEvent.setup();
    const { onSubmit } = renderForm();

    await fillNameAndPrompt(user);
    await user.selectOptions(screen.getByLabelText(/Trigger/), "interval");
    const seconds = screen.getByDisplayValue("3600");
    await user.clear(seconds);
    await user.type(seconds, "nope");
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("leaves the signal source open by default and closes on cancel", async () => {
    const user = userEvent.setup();
    const { onSubmit, onClose } = renderForm();

    await fillNameAndPrompt(user);
    await user.selectOptions(screen.getByLabelText(/Trigger/), "signal");
    await user.type(screen.getByPlaceholderText("deploy.finished"), "ci.failed");
    await user.click(screen.getByRole("button", { name: "Create routine" }));
    expect(onSubmit).toHaveBeenCalledWith(
      "nightly",
      { kind: "signal", name: "ci.failed" },
      "/nightly",
      "skip",
      NO_LIMITS,
    );

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });
});
