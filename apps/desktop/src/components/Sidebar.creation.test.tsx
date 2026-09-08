import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import * as fx from "../test/fixtures";
import { renderSidebar } from "../test/sidebar";

/**
 * Creating a bot is one click with no form behind it, so what the sidebar
 * shows — that a request is in flight — is all the user has to go on.
 */
describe("Sidebar bot creation", () => {
  it("creates a bot in a project from that project's own menu", async () => {
    const user = userEvent.setup();
    const props = renderSidebar({
      projects: [fx.project(), fx.project({ id: "p2", name: "Beta" })],
      selection: { kind: "project", projectId: "p2" },
    });

    await user.click(screen.getAllByRole("button", { name: "Project menu" })[0]);
    await user.click(screen.getByRole("menuitem", { name: "New Bot" }));

    expect(props.onCreateBot).toHaveBeenCalledWith("p1");
    expect(screen.queryByPlaceholderText("Bot name")).not.toBeInTheDocument();
  });

  it("marks the creation in flight and ignores a second click until it lands", async () => {
    const user = userEvent.setup();
    // Held open so the click can be observed mid-flight, then let go.
    const inFlight: Array<() => void> = [];
    const onCreateBot = vi.fn<(projectId: string) => Promise<void>>(
      () =>
        new Promise<void>((resolve) => {
          inFlight.push(resolve);
        }),
    );
    renderSidebar({ onCreateBot });

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    await user.click(screen.getByRole("menuitem", { name: "New Bot" }));
    expect(screen.getByRole("status")).toHaveTextContent("Creating bot");

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    await user.click(screen.getByRole("menuitem", { name: "New Bot" }));
    expect(onCreateBot).toHaveBeenCalledTimes(1);

    for (const resolve of inFlight) {
      resolve();
    }
    await waitFor(() => {
      expect(screen.queryByRole("status")).not.toBeInTheDocument();
    });
  });
});
