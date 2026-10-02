import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ProjectRepo } from "../protocol/entities";
import ProjectRepoForm from "./ProjectRepoForm";

function renderForm(repo: ProjectRepo | null = null, disabled = false) {
  const onSave = vi.fn<(repo: ProjectRepo | null) => Promise<void>>(() => Promise.resolve());
  render(<ProjectRepoForm repo={repo} disabled={disabled} onSave={onSave} />);
  return { onSave };
}

describe("ProjectRepoForm", () => {
  it("saves a new repository, defaulting the branch to main", async () => {
    const user = userEvent.setup();
    const { onSave } = renderForm();
    const save = screen.getByRole("button", { name: "Save repository" });
    expect(save).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Remove" })).toBeNull();

    await user.type(screen.getByLabelText("Clone URL"), " git@host:me/book.git ");
    await user.clear(screen.getByLabelText("Branch"));
    await user.click(save);

    expect(onSave).toHaveBeenCalledWith({ url: "git@host:me/book.git", branch: "main" });
  });

  it("saves only a change, and removes a repository", async () => {
    const user = userEvent.setup();
    const { onSave } = renderForm({ url: "git@host:me/book.git", branch: "main" });
    const save = screen.getByRole("button", { name: "Save repository" });
    expect(save).toBeDisabled();

    await user.clear(screen.getByLabelText("Branch"));
    await user.type(screen.getByLabelText("Branch"), "drafts");
    await user.click(save);
    expect(onSave).toHaveBeenLastCalledWith({ url: "git@host:me/book.git", branch: "drafts" });

    await user.click(screen.getByRole("button", { name: "Remove" }));
    expect(onSave).toHaveBeenLastCalledWith(null);
  });

  it("is read-only without control", () => {
    renderForm({ url: "u", branch: "main" }, true);
    expect(screen.getByLabelText("Clone URL")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove" })).toBeDisabled();
  });
});
