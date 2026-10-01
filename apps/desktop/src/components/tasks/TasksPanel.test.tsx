import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import * as fx from "../../test/fixtures";
import { sampleTasks, task, tasksDaemon } from "../../test/taskFixtures";
import TasksPanel, { sections } from "./TasksPanel";

describe("TasksPanel", () => {
  it("splits tasks into now, waiting, upcoming and done", async () => {
    render(<TasksPanel client={tasksDaemon()} bot={fx.bot()} connected />);
    const now = await screen.findByRole("region", { name: "Now" });
    expect(within(now).getByText("From lead")).toBeInTheDocument();
    const waiting = screen.getByRole("region", { name: "Waiting on others" });
    expect(within(waiting).getByText("To windev @ win-pc")).toBeInTheDocument();
    const upcoming = screen.getByRole("region", { name: "Upcoming" });
    expect(within(upcoming).getByText("Routine nightly-report")).toBeInTheDocument();
    expect(within(upcoming).queryByText("Routine paused")).not.toBeInTheDocument();
    const done = screen.getByRole("region", { name: "Done" });
    expect(within(done).getByText("done")).toBeInTheDocument();
    expect(within(done).getByText("expired")).toBeInTheDocument();
  });

  it("shows a finished task's result when opened", async () => {
    render(<TasksPanel client={tasksDaemon()} bot={fx.bot()} connected />);
    const row = await screen.findByText("Review the installer changes.");
    expect(screen.queryByText("Reviewed — two nits, both fixed.")).not.toBeInTheDocument();
    const more = within(row.closest("li") ?? document.body).getByRole("button", {
      name: "Show more",
    });
    await userEvent.click(more);
    expect(screen.getByText("Reviewed — two nits, both fixed.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Show less" }));
    expect(screen.queryByText("Reviewed — two nits, both fixed.")).not.toBeInTheDocument();
  });

  it("loads the whole task when its preview was cut, and shows it as markdown", async () => {
    const preview = task({
      id: "t9",
      state: "done",
      request: "Batch 15 review returned three findings…",
      request_truncated: true,
      result: "I ruled on all six items…",
      result_truncated: true,
    });
    const client = tasksDaemon([preview]).onRequest("get_task", () => ({
      type: "task",
      req_id: "9",
      task: {
        ...preview,
        request: "Batch 15 review returned three findings, all fixed.",
        request_truncated: false,
        result: "I ruled on **all six** items, guard order included.",
        result_truncated: false,
      },
    }));
    render(<TasksPanel client={client} bot={fx.bot()} connected />);
    await userEvent.click(await screen.findByRole("button", { name: "Show more" }));
    expect(await screen.findByText("all six")).toBeInTheDocument();
    expect(screen.getByText(/guard order included/)).toBeInTheDocument();
    expect(
      screen.getByText("Batch 15 review returned three findings, all fixed."),
    ).toBeInTheDocument();
    expect(client.requests.at(-1)?.body).toEqual({ type: "get_task", bot_id: "b1", task_id: "t9" });
  });

  it("refreshes after bus traffic and says when there is nothing", async () => {
    const client = tasksDaemon([]).onRequest("list_routines", () => ({
      type: "routines",
      req_id: "2",
      routines: [],
    }));
    render(<TasksPanel client={client} bot={fx.bot()} connected />);
    expect(await screen.findByText(/No tasks yet/)).toBeInTheDocument();

    client.onRequest("list_tasks", () => ({
      type: "tasks",
      req_id: "3",
      bot_id: "b1",
      tasks: [task()],
    }));
    act(() => {
      client.emit("message_new", { type: "message_new", message: fx.message() });
    });
    await waitFor(
      () => {
        expect(screen.getByRole("region", { name: "Now" })).toBeInTheDocument();
      },
      { timeout: 2000 },
    );
  });

  it("reports a daemon that cannot list tasks", async () => {
    const client = tasksDaemon().onRequest("list_tasks", () => {
      throw new Error("unknown type: list_tasks");
    });
    render(<TasksPanel client={client} bot={fx.bot()} connected />);
    expect(await screen.findByText("unknown type: list_tasks")).toBeInTheDocument();
  });

  it("groups by state and role", () => {
    const grouped = sections(sampleTasks);
    expect(grouped.now.map((t) => t.id)).toEqual(["t1"]);
    expect(grouped.waiting.map((t) => t.id)).toEqual(["t2"]);
    expect(grouped.done.map((t) => t.id)).toEqual(["t3", "t4"]);
  });
});
