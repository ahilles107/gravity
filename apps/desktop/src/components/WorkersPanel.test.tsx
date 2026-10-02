import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { WorkerView } from "../protocol/workers";
import { FakeDaemon } from "../test/fakeDaemon";
import WorkersPanel, { workerSections } from "./WorkersPanel";

function worker(over: Partial<WorkerView> = {}): WorkerView {
  return {
    id: "w1",
    project_id: "p1",
    name: "ch-1",
    state: "running",
    machine: "here",
    parent_bot_id: "b1",
    parent_name: "book",
    brief: "Write chapter one.",
    created_at: "2025-01-15T09:00:00Z",
    started_at: "2025-01-15T09:00:05Z",
    ...over,
  };
}

const sample: readonly WorkerView[] = [
  worker(),
  worker({ id: "w2", name: "ch-2", machine: "win-pc" }),
  worker({ id: "w3", name: "ch-3", state: "queued", queue_position: 1, started_at: null }),
  worker({ id: "w4", name: "ch-0", state: "done", finished_at: "2025-01-15T08:00:00Z" }),
];

function daemon(workers: readonly WorkerView[] = sample): FakeDaemon {
  return new FakeDaemon().onRequest("list_workers", () => ({
    type: "workers",
    req_id: "1",
    project_id: "p1",
    workers,
    running_here: 1,
    max_workers_here: 4,
  }));
}

describe("WorkersPanel", () => {
  it("shows what runs, what is queued and what finished", async () => {
    render(<WorkersPanel client={daemon()} projectId="p1" connected canControl />);
    const running = await screen.findByRole("region", { name: "Running" });
    expect(within(running).getByText("ch-1")).toBeInTheDocument();
    expect(within(running).getByText("running on win-pc")).toBeInTheDocument();
    const queued = screen.getByRole("region", { name: "Queued" });
    expect(within(queued).getByText("#1 in queue")).toBeInTheDocument();
    expect(within(queued).getByText("Write chapter one.")).toBeInTheDocument();
    const finished = screen.getByRole("region", { name: "Finished" });
    expect(within(finished).getByText("done")).toBeInTheDocument();
    expect(within(finished).queryByRole("button", { name: /Cancel/ })).toBeNull();
    expect(screen.getByText(/1 of 4 slots in use here/)).toBeInTheDocument();
  });

  it("cancels a spawn as the owner", async () => {
    const client = daemon().onRequest("cancel_worker", () => ({
      type: "worker",
      req_id: "2",
      worker: worker({ id: "w3", name: "ch-3", state: "cancelled" }),
    }));
    render(<WorkersPanel client={client} projectId="p1" connected canControl />);
    await userEvent.click(await screen.findByRole("button", { name: "Cancel ch-3" }));
    expect(client.requests.map((r) => r.body)).toContainEqual({
      type: "cancel_worker",
      worker_id: "w3",
    });
  });

  it("refetches when its project's queue changes", async () => {
    const client = daemon([]);
    render(<WorkersPanel client={client} projectId="p1" connected canControl={false} />);
    expect(await screen.findByText(/No workers yet/)).toBeInTheDocument();
    client.onRequest("list_workers", () => ({
      type: "workers",
      req_id: "3",
      project_id: "p1",
      workers: sample,
      running_here: 1,
      max_workers_here: 4,
    }));
    act(() => {
      client.emit("workers_updated", { type: "workers_updated", project_id: "other" });
    });
    expect(screen.queryByText("ch-1")).toBeNull();
    act(() => {
      client.emit("workers_updated", { type: "workers_updated", project_id: "p1" });
    });
    expect(await screen.findByText("ch-1")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Cancel ch-1" })).toBeDisabled();
    });
  });

  it("splits by state", () => {
    const { running, queued, finished } = workerSections(sample);
    expect([running.length, queued.length, finished.length]).toEqual([2, 1, 1]);
  });
});
