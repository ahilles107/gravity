import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { FakeDaemon } from "../test/fakeDaemon";
import * as fx from "../test/fixtures";
import { routinesSpy, toastSpy } from "../test/spies";
import RoutinesPanel from "./RoutinesPanel";

function daemonWith(routines: readonly ReturnType<typeof fx.routine>[]): FakeDaemon {
  return new FakeDaemon().onRequest("list_routines", () => ({
    type: "routines",
    req_id: "1",
    routines,
  }));
}

function renderPanel(daemon: FakeDaemon, canControl = true) {
  const onRoutinesChanged = routinesSpy();
  const onToast = toastSpy();
  render(
    <RoutinesPanel
      client={daemon}
      bot={fx.bot()}
      bots={[fx.bot(), fx.bot({ id: "b2", name: "bob" })]}
      connected
      canControl={canControl}
      onRoutinesChanged={onRoutinesChanged}
      onToast={onToast}
    />,
  );
  return { onRoutinesChanged, onToast };
}

describe("RoutinesPanel", () => {
  it("lists routines with their trigger and next run", async () => {
    const { onRoutinesChanged } = renderPanel(daemonWith([fx.routine()]));
    await waitFor(() => {
      expect(screen.getByText("weekly-report")).toBeInTheDocument();
    });
    expect(screen.getByText("cron 0 0 9 * * MON (Europe/Warsaw)")).toBeInTheDocument();
    expect(screen.getByText(/^next /)).toBeInTheDocument();
    expect(onRoutinesChanged).toHaveBeenCalledWith("b1", [fx.routine()]);
  });

  it("says when a routine has no next run", async () => {
    renderPanel(daemonWith([fx.routine({ next_run_at: null })]));
    await waitFor(() => {
      expect(screen.getByText("no next run")).toBeInTheDocument();
    });
  });

  it("shows the empty state", async () => {
    renderPanel(daemonWith([]));
    await waitFor(() => {
      expect(screen.getByText("No routines yet.")).toBeInTheDocument();
    });
  });

  it("reports a load failure", async () => {
    const daemon = new FakeDaemon().onRequest("list_routines", () => {
      throw new Error("offline");
    });
    const { onToast } = renderPanel(daemon);
    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("error", "Failed to load routines", "offline");
    });
  });

  it("toggles a routine and surfaces failures", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([fx.routine()]).onRequest("set_routine_enabled", () => ({
      type: "routine",
      req_id: "1",
      routine: fx.routine({ enabled: false }),
    }));
    renderPanel(daemon);

    await waitFor(() => {
      expect(screen.getByRole("checkbox")).toBeChecked();
    });
    await user.click(screen.getByRole("checkbox"));
    await waitFor(() => {
      expect(screen.getByRole("checkbox")).not.toBeChecked();
    });

    const failing = daemonWith([fx.routine()]).onRequest("set_routine_enabled", () => {
      throw new Error("locked");
    });
    const { onToast } = renderPanel(failing);
    await waitFor(() => {
      expect(screen.getAllByRole("checkbox")).toHaveLength(2);
    });
    await user.click(screen.getAllByRole("checkbox")[1] as HTMLElement);
    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("error", "Failed to update routine", "locked");
    });
  });

  it("runs a routine now", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([fx.routine()]).onRequest("run_routine_now", () => ({
      type: "ok",
      req_id: "1",
    }));
    const { onToast } = renderPanel(daemon);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Run now" })).toBeInTheDocument();
    });
    await user.click(screen.getByRole("button", { name: "Run now" }));

    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("info", "Routine queued", expect.any(String));
    });
  });

  it("surfaces a run-now failure", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([fx.routine()]).onRequest("run_routine_now", () => {
      throw new Error("busy");
    });
    const { onToast } = renderPanel(daemon);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Run now" })).toBeInTheDocument();
    });
    await user.click(screen.getByRole("button", { name: "Run now" }));

    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("error", "Run now failed", "busy");
    });
  });

  it("expands and collapses the run history", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([fx.routine()]).onRequest("list_routine_runs", () => ({
      type: "routine_runs",
      req_id: "1",
      routine_runs: [fx.routineRun({ started_at: "2024-05-01T09:00:00.000Z" })],
    }));
    renderPanel(daemon);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "History" })).toBeInTheDocument();
    });
    await user.click(screen.getByRole("button", { name: "History" }));

    await waitFor(() => {
      expect(screen.getByText("succeeded")).toBeInTheDocument();
    });
    await user.click(screen.getByRole("button", { name: "Hide history" }));
    expect(screen.queryByText("succeeded")).not.toBeInTheDocument();
  });

  it("shows history errors and the empty history state", async () => {
    const user = userEvent.setup();
    const empty = daemonWith([fx.routine()]).onRequest("list_routine_runs", () => ({
      type: "routine_runs",
      req_id: "1",
      routine_runs: [],
    }));
    renderPanel(empty);
    await user.click(await screen.findByRole("button", { name: "History" }));
    await waitFor(() => {
      expect(screen.getByText("No runs yet.")).toBeInTheDocument();
    });

    const failing = daemonWith([fx.routine()]).onRequest("list_routine_runs", () => {
      throw new Error("no history");
    });
    renderPanel(failing);
    const buttons = await screen.findAllByRole("button", { name: "History" });
    await user.click(buttons[buttons.length - 1] as HTMLElement);
    await waitFor(() => {
      expect(screen.getByText("Failed to load runs: no history")).toBeInTheDocument();
    });
  });

  it("live-updates run history from pushes", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([fx.routine()]).onRequest("list_routine_runs", () => ({
      type: "routine_runs",
      req_id: "1",
      routine_runs: [],
    }));
    renderPanel(daemon);
    await user.click(await screen.findByRole("button", { name: "History" }));
    await waitFor(() => {
      expect(screen.getByText("No runs yet.")).toBeInTheDocument();
    });

    daemon.emit("routine_run_update", {
      type: "routine_run_update",
      routine_run: fx.routineRun({ id: "rr2", state: "running" }),
    });
    await waitFor(() => {
      expect(screen.getByText("running")).toBeInTheDocument();
    });

    daemon.emit("routine_run_update", {
      type: "routine_run_update",
      routine_run: fx.routineRun({ id: "rrX", routine_id: "other" }),
    });
    expect(screen.getAllByRole("row")).toHaveLength(2);
  });

  it("keeps only the newest twenty live run updates", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([fx.routine()]).onRequest("list_routine_runs", () => ({
      type: "routine_runs",
      req_id: "1",
      routine_runs: [],
    }));
    renderPanel(daemon);
    await user.click(await screen.findByRole("button", { name: "History" }));

    act(() => {
      for (let index = 0; index < 25; index += 1) {
        daemon.emit("routine_run_update", {
          type: "routine_run_update",
          routine_run: fx.routineRun({ id: `rr-${index}`, state: "running" }),
        });
      }
    });

    await waitFor(() => {
      expect(screen.getAllByRole("row")).toHaveLength(21);
    });
  });

  it("does not offer the routine owner as its own signal source", async () => {
    const user = userEvent.setup();
    renderPanel(daemonWith([]));

    await user.click(await screen.findByText("+ New routine"));
    await user.selectOptions(screen.getByLabelText(/Trigger/), "signal");

    expect(screen.queryByRole("option", { name: "alice" })).not.toBeInTheDocument();
    expect(screen.getByRole("option", { name: "bob" })).toBeInTheDocument();
  });

  it("creates a routine", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([]).onRequest("create_routine", () => ({
      type: "routine",
      req_id: "1",
      routine: fx.routine({ id: "r9", name: "nightly" }),
    }));
    renderPanel(daemon);

    await user.click(await screen.findByText("+ New routine"));
    await user.type(screen.getByPlaceholderText("weekly-report"), "nightly");
    await user.type(screen.getByPlaceholderText("/weekly-report"), "/nightly");
    await user.click(screen.getByRole("button", { name: "Create routine" }));

    await waitFor(() => {
      expect(screen.getByText("nightly")).toBeInTheDocument();
    });
    const created = daemon.requests.find((r) => r.body.type === "create_routine");
    expect(created?.body).toMatchObject({ name: "nightly", overlap_policy: "skip" });
  });

  it("ignores an incomplete new routine and can be cancelled", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([]);
    renderPanel(daemon);

    await user.click(await screen.findByText("+ New routine"));
    await user.click(screen.getByRole("button", { name: "Create routine" }));
    expect(daemon.requests.some((r) => r.body.type === "create_routine")).toBe(false);

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByPlaceholderText("weekly-report")).not.toBeInTheDocument();
  });

  it("hides mutations without the control grant", async () => {
    renderPanel(daemonWith([fx.routine()]), false);
    await waitFor(() => {
      expect(screen.getByText("weekly-report")).toBeInTheDocument();
    });
    expect(screen.queryByText("+ New routine")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Run now" })).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox")).toBeDisabled();
  });
});
