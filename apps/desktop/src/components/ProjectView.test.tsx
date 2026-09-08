import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import * as fx from "../test/fixtures";
import ProjectView from "./ProjectView";

function renderView(over: Partial<Parameters<typeof ProjectView>[0]> = {}) {
  const onRename = vi.fn<(projectId: string, name: string) => Promise<void>>(() =>
    Promise.resolve(),
  );
  const onDelete = vi.fn<(projectId: string) => Promise<void>>(() => Promise.resolve());
  render(
    <ProjectView
      project={fx.project()}
      bots={[fx.bot()]}
      connected
      canControl
      onRename={onRename}
      onDelete={onDelete}
      {...over}
    />,
  );
  return { onRename, onDelete };
}

describe("ProjectView", () => {
  it("saves only a changed name", async () => {
    const user = userEvent.setup();
    const { onRename } = renderView();

    const save = screen.getByRole("button", { name: "Save" });
    expect(save).toBeDisabled();

    await user.clear(screen.getByLabelText("Name"));
    await user.type(screen.getByLabelText("Name"), "Initech");
    await user.click(save);

    expect(onRename).toHaveBeenCalledWith("p1", "Initech");
  });

  it("does not offer to save whitespace", async () => {
    const user = userEvent.setup();
    renderView();

    await user.clear(screen.getByLabelText("Name"));
    await user.type(screen.getByLabelText("Name"), "   ");

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("confirms a delete before running it, and counts what goes with it", async () => {
    const user = userEvent.setup();
    const { onDelete } = renderView({ bots: [fx.bot(), fx.bot({ id: "b2", name: "bob" })] });

    await user.click(screen.getByRole("button", { name: "Delete project" }));
    const dialog = screen.getByRole("dialog", { name: "Delete project" });
    expect(within(dialog).getByText(/2 bots are stopped and archived/)).toBeInTheDocument();
    expect(onDelete).not.toHaveBeenCalled();

    await user.click(within(dialog).getByRole("button", { name: "Delete project" }));
    expect(onDelete).toHaveBeenCalledWith("p1");
  });

  /** A read-only connection may look at a project but must not reshape it. */
  it("hides deletion and locks the name without control", () => {
    renderView({ canControl: false });

    expect(screen.getByLabelText("Name")).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Delete project" })).not.toBeInTheDocument();
  });
});
