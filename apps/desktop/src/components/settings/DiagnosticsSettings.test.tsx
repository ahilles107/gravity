import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import * as fx from "../../test/fixtures";
import { toastSpy } from "../../test/spies";
import DiagnosticsSettings from "./DiagnosticsSettings";

function baseDaemon(): FakeDaemon {
  return new FakeDaemon()
    .onRequest("diagnostics", () => ({
      type: "diagnostics",
      req_id: "1",
      diagnostics: fx.diagnostics(),
    }))
    .onRequest("list_deliveries", () => ({
      type: "deliveries",
      req_id: "1",
      deliveries: [fx.delivery({ id: "d1", state: "failed", last_error: "timeout" })],
    }));
}

function renderPane(daemon: FakeDaemon, canControl = true) {
  const onToast = toastSpy();
  render(
    <DiagnosticsSettings
      client={daemon}
      bots={[fx.bot({ id: "b1", name: "alice" })]}
      connected
      canControl={canControl}
      onToast={onToast}
    />,
  );
  return { onToast };
}

describe("DiagnosticsSettings", () => {
  it("renders the daemon summary", async () => {
    renderPane(baseDaemon());
    await waitFor(() => {
      expect(screen.getByText("healthy")).toBeInTheDocument();
    });
    expect(screen.getByText("claude 1.2.3 (available)")).toBeInTheDocument();
    expect(screen.getByText("0.1.0-test")).toBeInTheDocument();
  });

  it("reports a load failure", async () => {
    const daemon = baseDaemon().onRequest("diagnostics", () => {
      throw new Error("no daemon");
    });
    renderPane(daemon);
    await waitFor(() => {
      expect(screen.getByText("Failed to load: no daemon")).toBeInTheDocument();
    });
  });

  it("lists deliveries with the bot name and retries a failed one", async () => {
    const user = userEvent.setup();
    const daemon = baseDaemon().onRequest("retry_delivery", () => ({ type: "ok", req_id: "1" }));
    const { onToast } = renderPane(daemon);

    await waitFor(() => {
      expect(screen.getByText("alice")).toBeInTheDocument();
    });
    expect(screen.getByText("timeout")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("info", "Delivery retried", expect.any(String));
    });
  });

  it("hides retry without the control grant", async () => {
    renderPane(baseDaemon(), false);
    await waitFor(() => {
      expect(screen.getByText("alice")).toBeInTheDocument();
    });
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  });

  it("prepends deliveries pushed by the daemon", async () => {
    const daemon = baseDaemon();
    renderPane(daemon);
    await waitFor(() => {
      expect(screen.getByText("alice")).toBeInTheDocument();
    });

    daemon.emit("delivery_update", {
      type: "delivery_update",
      delivery: fx.delivery({ id: "d2", bot_id: "ghost", state: "queued" }),
    });

    await waitFor(() => {
      expect(screen.getByText("ghost")).toBeInTheDocument();
    });
  });

  it("reloads on demand", async () => {
    const user = userEvent.setup();
    const daemon = baseDaemon();
    renderPane(daemon);
    await waitFor(() => {
      expect(screen.getByText("healthy")).toBeInTheDocument();
    });
    const before = daemon.requests.filter((r) => r.body.type === "diagnostics").length;

    await user.click(screen.getByRole("button", { name: "Refresh" }));

    await waitFor(() => {
      expect(daemon.requests.filter((r) => r.body.type === "diagnostics").length).toBe(before + 1);
    });
  });
});
