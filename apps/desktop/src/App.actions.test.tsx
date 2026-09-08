import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { FakeDaemon } from "./test/fakeDaemon";
import * as fx from "./test/fixtures";

let daemon: FakeDaemon;

vi.mock("./components/TerminalPane", () => ({
  default: (): React.ReactElement => <div data-testid="terminal" />,
}));
// `new DaemonClient(...)` yields the prepared double: a constructor that
// returns an object produces that object.
vi.mock("./protocol/client", () => ({
  DaemonClient: function DaemonClientDouble(): FakeDaemon {
    return daemon;
  },
}));
vi.mock("./token", () => ({ readClientToken: () => Promise.resolve("token") }));

function seedDaemon(): FakeDaemon {
  return new FakeDaemon()
    .onRequest("list_projects", () => ({ type: "projects", req_id: "1", projects: [fx.project()] }))
    .onRequest("list_bots", () => ({
      type: "bots",
      req_id: "1",
      bots: [fx.bot({ unread_count: 2 })],
    }))
    .onRequest("list_conversations", () => ({
      type: "conversations",
      req_id: "1",
      conversations: [fx.conversation()],
    }))
    .onRequest("list_deliveries", () => ({ type: "deliveries", req_id: "1", deliveries: [] }))
    .onRequest("list_routines", () => ({ type: "routines", req_id: "1", routines: [fx.routine()] }))
    .onRequest("list_messages", () => ({ type: "messages", req_id: "1", messages: [] }));
}

/** Renders the app and waits for the initial snapshot to land. */
async function renderApp(): Promise<void> {
  render(<App />);
  daemon.setStatus("connected");
  await waitFor(() => {
    expect(screen.getByText("Acme")).toBeInTheDocument();
  });
  // The snapshot paints one commit before the startup pick runs, so flush that
  // effect too - otherwise assertions race an empty main pane.
  await act(async () => {});
}

describe("App actions", () => {
  beforeEach(() => {
    daemon = seedDaemon();
    daemon.status = "disconnected";
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("walks a fresh daemon through creating the first project", async () => {
    const user = userEvent.setup();
    daemon
      .onRequest("list_projects", () => ({ type: "projects", req_id: "1", projects: [] }))
      .onRequest("list_bots", () => ({ type: "bots", req_id: "1", bots: [] }))
      .onRequest("list_conversations", () => ({
        type: "conversations",
        req_id: "1",
        conversations: [],
      }))
      .onRequest("create_project", () => ({ type: "project", req_id: "1", project: fx.project() }))
      .onRequest("create_bot", () => ({ type: "bot", req_id: "1", bot: fx.bot() }));
    render(<App />);
    daemon.setStatus("connected");

    // renderApp waits for the seeded project row, which a fresh daemon lacks.
    await waitFor(() => {
      expect(screen.getByText("Welcome to Gravity")).toBeInTheDocument();
    });
    // The post-create refresh must see what the daemon just made.
    daemon
      .onRequest("list_projects", () => ({
        type: "projects",
        req_id: "1",
        projects: [fx.project()],
      }))
      .onRequest("list_bots", () => ({ type: "bots", req_id: "1", bots: [fx.bot()] }));
    await user.type(screen.getByPlaceholderText("Project name"), "athens");
    await user.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => {
      expect(
        daemon.requests.some(
          (request) => request.body.type === "create_project" && request.body.name === "athens",
        ),
      ).toBe(true);
    });
    // The first bot rides along and its view opens, not another empty pane.
    await waitFor(() => {
      expect(
        daemon.requests.some(
          (request) => request.body.type === "create_bot" && request.body.project_id === "p1",
        ),
      ).toBe(true);
    });
    expect(screen.getByTestId("terminal")).toBeInTheDocument();
  });

  it("creates a project and a bot through the sidebar", async () => {
    const user = userEvent.setup();
    const newBot = fx.bot({ id: "b2", name: "New Bot", dir_name: "new-bot" });
    daemon
      .onRequest("create_project", () => ({ type: "project", req_id: "1", project: fx.project() }))
      .onRequest("create_bot", () => ({ type: "bot", req_id: "1", bot: newBot }));
    await renderApp();

    await user.click(screen.getByTitle("New Project"));
    await user.type(screen.getByPlaceholderText("Project name"), "Beta");
    await user.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => {
      expect(daemon.requests.some((r) => r.body.type === "create_project")).toBe(true);
    });
    daemon.onRequest("list_bots", () => ({
      type: "bots",
      req_id: "1",
      bots: [fx.bot(), newBot],
    }));

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    await user.click(screen.getByRole("menuitem", { name: "New Bot" }));
    await waitFor(() => {
      expect(screen.getByText("New Bot", { selector: ".view-title" })).toBeInTheDocument();
    });
    const creation = daemon.requests.find((request) => request.body.type === "create_bot");
    expect(creation?.body).toEqual({ type: "create_bot", project_id: "p1" });
    expect(screen.getByTestId("terminal")).toBeInTheDocument();
  });

  it("deletes a bot from the sidebar context menu", async () => {
    const user = userEvent.setup();
    daemon.onRequest("delete_bot", () => ({ type: "ok", req_id: "1" }));
    await renderApp();

    await user.pointer({
      keys: "[MouseRight]",
      target: screen.getByText("alice", { selector: ".bot-row-name" }),
    });
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));
    await user.click(screen.getByRole("button", { name: "Delete bot" }));

    await waitFor(() => {
      expect(daemon.requests.some((r) => r.body.type === "delete_bot")).toBe(true);
    });
  });

  it("reports delete failures", async () => {
    const user = userEvent.setup();
    daemon.onRequest("delete_bot", () => {
      throw new Error("bot not found");
    });
    await renderApp();

    await user.pointer({
      keys: "[MouseRight]",
      target: screen.getByText("alice", { selector: ".bot-row-name" }),
    });
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));
    await user.click(screen.getByRole("button", { name: "Delete bot" }));

    await waitFor(() => {
      expect(screen.getByText("Delete bot failed")).toBeInTheDocument();
    });
  });

  it("reports create failures", async () => {
    const user = userEvent.setup();
    daemon.onRequest("create_project", () => {
      throw new Error("duplicate");
    });
    await renderApp();

    await user.click(screen.getByTitle("New Project"));
    await user.type(screen.getByPlaceholderText("Project name"), "Acme");
    await user.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => {
      expect(screen.getByText("Create project failed")).toBeInTheDocument();
    });
  });

  it("opens the palette with ⌘K and navigates to a bot", async () => {
    const user = userEvent.setup();
    await renderApp();

    await user.keyboard("{Meta>}k{/Meta}");
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();

    await user.click(screen.getByText("Diagnostics", { selector: ".overlay-label" }));
    await waitFor(() => {
      expect(screen.getByText("Deliveries")).toBeInTheDocument();
    });
  });

  it("closes the palette on a second ⌘K", async () => {
    const user = userEvent.setup();
    await renderApp();

    await user.keyboard("{Control>}k{/Control}");
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
    await user.keyboard("{Control>}k{/Control}");
    expect(screen.queryByRole("dialog", { name: "Command palette" })).not.toBeInTheDocument();
  });

  it("searches from the sidebar and opens the matching conversation", async () => {
    const user = userEvent.setup();
    daemon.onRequest("search", () => ({
      type: "search_results",
      req_id: "1",
      search_results: [fx.message({ body: "standup notes" })],
    }));
    await renderApp();

    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.type(screen.getByPlaceholderText("Search bots and messages…"), "notes");
    await waitFor(() => {
      expect(screen.getByText("standup notes")).toBeInTheDocument();
    });

    await user.click(screen.getByText("standup notes"));
    await waitFor(() => {
      expect(screen.getByTestId("terminal")).toBeInTheDocument();
    });
  });

  it("warns when a search result points at a vanished conversation", async () => {
    const user = userEvent.setup();
    daemon.onRequest("search", () => ({
      type: "search_results",
      req_id: "1",
      search_results: [fx.message({ conversation_id: "ghost", body: "orphan" })],
    }));
    await renderApp();

    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.type(screen.getByPlaceholderText("Search bots and messages…"), "orphan");
    await user.click(await screen.findByText("orphan"));

    await waitFor(() => {
      expect(screen.getByText("Conversation not found")).toBeInTheDocument();
    });
  });

  it("changes the daemon endpoint from connection settings", async () => {
    const user = userEvent.setup();
    await renderApp();

    await user.click(screen.getByTitle("Connection settings"));
    const host = screen.getByLabelText("Daemon host");
    await user.clear(host);
    await user.type(host, "mini");
    await user.click(screen.getByRole("button", { name: "Connect" }));

    await waitFor(() => {
      expect(daemon.getEndpoint()).toEqual({ host: "mini", port: 7777 });
    });
  });

  it("opens a bot from the command palette and offers no lifecycle entries", async () => {
    const user = userEvent.setup();
    await renderApp();

    await user.keyboard("{Meta>}k{/Meta}");
    expect(screen.queryByText("Stop alice")).not.toBeInTheDocument();
    expect(screen.queryByText("Start alice")).not.toBeInTheDocument();

    await user.click(screen.getByText("alice", { selector: ".overlay-label" }));
    await waitFor(() => {
      expect(screen.getByText("alice", { selector: ".view-title" })).toBeInTheDocument();
    });
    expect(screen.queryByRole("button", { name: "Stop" })).not.toBeInTheDocument();
  });
});
