import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as fx from "../test/fixtures";
import { renderSidebar } from "../test/sidebar";
import { stubLocalStorage } from "../test/spies";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("Sidebar", () => {
  it("lists projects with their bots", () => {
    renderSidebar();
    expect(screen.getByText("Acme")).toBeInTheDocument();
    expect(screen.getByText("alice")).toBeInTheDocument();
  });

  it("makes the header a window drag strip", () => {
    renderSidebar();
    const header = screen.getByRole("button", { name: "New Project" }).closest(".sidebar-header");
    expect(header).toHaveAttribute("data-tauri-drag-region", "deep");
  });

  it("shows the empty states", () => {
    renderSidebar({ projects: [], bots: [] });
    expect(screen.getByText("No projects. Create one to begin.")).toBeInTheDocument();

    renderSidebar({ bots: [] });
    expect(screen.getByText("No bots yet.")).toBeInTheDocument();
  });

  it("previews what the bot last said", () => {
    renderSidebar({ activityByBot: { b1: fx.botActivity() } });
    expect(screen.getByText("rules locked in")).toBeInTheDocument();
  });

  it("prefixes a preview the bot did not say", () => {
    renderSidebar({ activityByBot: { b1: fx.botActivity({ from: "you", text: "ping" }) } });
    expect(screen.getByText("you: ping")).toBeInTheDocument();
  });

  it("falls back to the description for a silent bot", () => {
    renderSidebar();
    expect(screen.getByText("does things")).toBeInTheDocument();
  });

  it("shows unread and failed-delivery badges", () => {
    renderSidebar({
      unreadBots: { b1: 3 },
      failedByBot: new Map([["b1", 2]]),
      nextRun: { b1: "2024-05-01T09:00:00.000Z" },
    });
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByTitle("2 failed deliveries")).toBeInTheDocument();
  });

  it("selects a bot", async () => {
    const user = userEvent.setup();
    const props = renderSidebar();

    await user.click(screen.getByText("alice"));

    expect(props.onSelect.mock.calls.map((call) => call[0])).toEqual([
      { kind: "bot", botId: "b1" },
    ]);
  });

  it("opens project settings from the header menu", async () => {
    const user = userEvent.setup();
    const props = renderSidebar();

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Project settings" }));

    expect(props.onSelect).toHaveBeenCalledWith({ kind: "project", projectId: "p1" });
  });

  it("collapses and expands the project's bots from its label", async () => {
    const user = userEvent.setup();
    renderSidebar();

    await user.click(screen.getByText("Acme"));
    expect(screen.queryByText("alice", { selector: ".bot-row-name" })).not.toBeInTheDocument();

    await user.click(screen.getByText("Acme"));
    expect(screen.getByText("alice", { selector: ".bot-row-name" })).toBeInTheDocument();
  });

  it("offers the same menu from the label's right-click and the ⋯ button", async () => {
    const user = userEvent.setup();
    renderSidebar();
    const entries = ["New Bot", "Project settings", "Reveal in Finder", "Delete project"];

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("Acme") });
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(entries);
    await user.keyboard("{Escape}");

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(entries);
  });

  it("deletes a project from its header menu, after confirming", async () => {
    const user = userEvent.setup();
    const props = renderSidebar();

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete project" }));

    const dialog = screen.getByRole("dialog", { name: "Delete project" });
    await user.click(within(dialog).getByRole("button", { name: "Delete project" }));

    expect(props.onDeleteProject).toHaveBeenCalledWith("p1");
  });

  it("deletes a project from its right-click menu, after confirming", async () => {
    const user = userEvent.setup();
    const props = renderSidebar();

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("Acme") });
    await user.click(screen.getByRole("menuitem", { name: "Delete project" }));
    expect(props.onDeleteProject).not.toHaveBeenCalled();

    const dialog = screen.getByRole("dialog", { name: "Delete project" });
    await user.click(within(dialog).getByRole("button", { name: "Delete project" }));

    expect(props.onDeleteProject).toHaveBeenCalledWith("p1");
  });

  /** Without control the menu still opens; it just has nothing destructive in it. */
  it("offers no project deletion without the control grant", async () => {
    const user = userEvent.setup();
    renderSidebar({ canControl: false });

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("Acme") });

    expect(screen.getByRole("menuitem", { name: "Project settings" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Delete project" })).not.toBeInTheDocument();
  });

  it("hides creation controls without the control grant", () => {
    renderSidebar({ canControl: false });
    expect(screen.queryByTitle("New Project")).not.toBeInTheDocument();
    expect(screen.queryByTitle("New bot")).not.toBeInTheDocument();
    expect(screen.getByText("read-only")).toBeInTheDocument();
  });

  it("creates a project from the header button and ignores a blank name", async () => {
    const user = userEvent.setup();
    const props = renderSidebar();

    await user.click(screen.getByTitle("New Project"));
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(props.onCreateProject).not.toHaveBeenCalled();

    await user.type(screen.getByPlaceholderText("Project name"), "Beta");
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(props.onCreateProject).toHaveBeenCalledWith("Beta");
  });

  it("opens settings from the footer, at the connection category from the endpoint line", async () => {
    const user = userEvent.setup();
    const props = renderSidebar();

    await user.click(screen.getByText("Settings"));
    expect(props.onOpenSettings).toHaveBeenCalledWith();

    await user.click(screen.getByTitle("Connection settings"));
    expect(props.onOpenSettings).toHaveBeenLastCalledWith("connection");
  });

  it("deletes a bot from the row context menu after confirming", async () => {
    const user = userEvent.setup();
    const props = renderSidebar();

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("alice") });
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));

    expect(screen.getByRole("dialog", { name: "Delete bot" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Delete bot" }));

    expect(props.onDeleteBot).toHaveBeenCalledWith("b1");
    expect(screen.queryByRole("dialog", { name: "Delete bot" })).not.toBeInTheDocument();
  });

  it("cancels the delete confirmation and leaves the bot alone", async () => {
    const user = userEvent.setup();
    const props = renderSidebar();

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("alice") });
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("dialog", { name: "Delete bot" })).not.toBeInTheDocument();
    expect(props.onDeleteBot).not.toHaveBeenCalled();
  });

  it("closes the context menu on Escape", async () => {
    const user = userEvent.setup();
    renderSidebar();

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("alice") });
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menuitem", { name: "Delete" })).not.toBeInTheDocument();
  });

  it("omits delete from the context menu without the control grant", async () => {
    const user = userEvent.setup();
    renderSidebar({ canControl: false });

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("alice") });
    expect(screen.queryByRole("menuitem", { name: "Delete" })).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Pin to top" })).toBeInTheDocument();
  });

  it("pins a bot to the top and unpins it from the tile", async () => {
    const user = userEvent.setup();
    const store = stubLocalStorage();
    const props = renderSidebar({ bots: [fx.bot(), fx.bot({ id: "b2", name: "bob" })] });

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("bob") });
    await user.click(screen.getByRole("menuitem", { name: "Pin to top" }));

    const tile = screen.getByText("bob").closest("button");
    expect(tile).toHaveClass("pin-tile");
    expect(store.get("gravity.pinned-bots")).toBe(JSON.stringify(["b2"]));

    await user.click(screen.getByText("bob"));
    expect(props.onSelect).toHaveBeenCalledWith({ kind: "bot", botId: "b2" });

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("bob") });
    await user.click(screen.getByRole("menuitem", { name: "Unpin" }));
    expect(screen.getByText("bob").closest("button")).toHaveClass("row");
  });

  it("restores pinned bots in their pinned order", () => {
    stubLocalStorage({ "gravity.pinned-bots": JSON.stringify(["b2", "b1"]) });
    renderSidebar({
      bots: [fx.bot(), fx.bot({ id: "b2", name: "bob" })],
      unreadBots: { b2: 4 },
    });

    const tiles = screen
      .getAllByRole("button")
      .filter((node) => node.classList.contains("pin-tile"));
    expect(tiles.map((node) => node.textContent)).toEqual(["B4bob", "Aalice"]);
  });

  it("opens search", async () => {
    const user = userEvent.setup();
    const props = renderSidebar();
    await user.click(screen.getByText("Search"));
    expect(props.onOpenSearch).toHaveBeenCalled();
  });

  it("renders each connection status label", () => {
    renderSidebar({ status: "connecting" });
    expect(screen.getByText(/connecting…/)).toBeInTheDocument();
    renderSidebar({ status: "auth_failed" });
    expect(screen.getByText(/auth failed/)).toBeInTheDocument();
  });
});
