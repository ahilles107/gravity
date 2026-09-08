import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { FakeDaemon } from "../test/fakeDaemon";
import * as fx from "../test/fixtures";
import { toastSpy } from "../test/spies";
import DevicesPanel from "./DevicesPanel";

function renderPanel(daemon: FakeDaemon, over: { connected?: boolean; canControl?: boolean } = {}) {
  const onToast = toastSpy();
  render(
    <DevicesPanel
      client={daemon}
      connected={over.connected ?? true}
      canControl={over.canControl ?? true}
      onToast={onToast}
    />,
  );
  return { onToast };
}

function daemonWith(devices: readonly ReturnType<typeof fx.device>[]): FakeDaemon {
  return new FakeDaemon().onRequest("list_devices", () => ({
    type: "devices",
    req_id: "1",
    devices,
  }));
}

describe("DevicesPanel", () => {
  it("lists devices and marks the current one", async () => {
    const daemon = daemonWith([
      fx.device({ id: "dev1", name: "laptop", last_seen_at: "2024-05-01T10:00:00.000Z" }),
      fx.device({ id: "dev2", name: "phone", revoked_at: "2024-05-02T10:00:00.000Z" }),
    ]);
    daemon.deviceId = "dev1";
    renderPanel(daemon);

    await waitFor(() => {
      expect(screen.getByText("laptop")).toBeInTheDocument();
    });
    expect(screen.getByText("(this device)")).toBeInTheDocument();
    expect(screen.getByText("active")).toBeInTheDocument();
    expect(screen.getByText("revoked")).toBeInTheDocument();
    expect(screen.getByText("never")).toBeInTheDocument();
  });

  it("shows the empty state", async () => {
    renderPanel(daemonWith([]));
    await waitFor(() => {
      expect(
        screen.getByText("No devices. Create one to connect another client."),
      ).toBeInTheDocument();
    });
  });

  it("reports a failed load", async () => {
    const daemon = new FakeDaemon().onRequest("list_devices", () => {
      throw new Error("db down");
    });
    renderPanel(daemon);
    await waitFor(() => {
      expect(screen.getByText("Failed to load devices: db down")).toBeInTheDocument();
    });
  });

  it("stays on Loading… while disconnected", () => {
    renderPanel(daemonWith([]), { connected: false });
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("creates a device and reveals the one-time token", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn<(text: string) => Promise<void>>(() => Promise.resolve());
    vi.stubGlobal("navigator", { clipboard: { writeText } });

    const daemon = daemonWith([]).onRequest("create_device", () => ({
      type: "device",
      req_id: "1",
      device: fx.device({ id: "dev9", name: "ci" }),
      token: "secret-token",
    }));
    const { onToast } = renderPanel(daemon);

    await user.click(screen.getByText("+ New device"));
    await user.type(screen.getByPlaceholderText("phone, laptop, ci-runner…"), "ci");
    await user.click(screen.getByLabelText(/control/));
    await user.click(screen.getByRole("button", { name: "Create device" }));

    await waitFor(() => {
      expect(screen.getByDisplayValue("secret-token")).toBeInTheDocument();
    });
    const created = daemon.requests.find((r) => r.body.type === "create_device");
    expect(created?.body).toMatchObject({ name: "ci", capabilities: ["read", "control"] });

    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(writeText).toHaveBeenCalledWith("secret-token");
    expect(onToast).toHaveBeenCalledWith("info", "Token copied", expect.any(String));

    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByDisplayValue("secret-token")).not.toBeInTheDocument();
  });

  it("warns when the daemon returns no token", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([]).onRequest("create_device", () => ({
      type: "device",
      req_id: "1",
      device: fx.device({ id: "dev9", name: "ci" }),
    }));
    const { onToast } = renderPanel(daemon);

    await user.click(screen.getByText("+ New device"));
    await user.type(screen.getByPlaceholderText("phone, laptop, ci-runner…"), "ci");
    await user.click(screen.getByRole("button", { name: "Create device" }));

    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("warn", "Device created", expect.any(String));
    });
  });

  it("surfaces a create failure", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([]).onRequest("create_device", () => {
      throw new Error("name taken");
    });
    const { onToast } = renderPanel(daemon);

    await user.click(screen.getByText("+ New device"));
    await user.type(screen.getByPlaceholderText("phone, laptop, ci-runner…"), "ci");
    await user.click(screen.getByRole("button", { name: "Create device" }));

    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("error", "Create device failed", "name taken");
    });
  });

  it("disables submission without a name or capability", async () => {
    const user = userEvent.setup();
    renderPanel(daemonWith([]));

    await user.click(screen.getByText("+ New device"));
    const submit = screen.getByRole("button", { name: "Create device" });
    expect(submit).toBeDisabled();

    await user.type(screen.getByPlaceholderText("phone, laptop, ci-runner…"), "ci");
    expect(submit).toBeEnabled();

    await user.click(screen.getByLabelText(/read/));
    expect(submit).toBeDisabled();
  });

  it("revokes a device behind a confirmation", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([fx.device({ id: "dev1", name: "laptop" })]).onRequest(
      "revoke_device",
      () => ({
        type: "device",
        req_id: "1",
        device: fx.device({ id: "dev1", name: "laptop", revoked_at: "2024-05-02T00:00:00.000Z" }),
      }),
    );
    const { onToast } = renderPanel(daemon);

    await waitFor(() => {
      expect(screen.getByText("laptop")).toBeInTheDocument();
    });
    await user.click(screen.getByRole("button", { name: "Revoke" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(daemon.requests.some((r) => r.body.type === "revoke_device")).toBe(false);

    await user.click(screen.getByRole("button", { name: "Revoke" }));
    await user.click(screen.getByRole("button", { name: "Confirm revoke" }));

    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("info", "Device revoked", expect.any(String));
    });
    expect(screen.getByText("revoked")).toBeInTheDocument();
  });

  it("surfaces a revoke failure", async () => {
    const user = userEvent.setup();
    const daemon = daemonWith([fx.device()]).onRequest("revoke_device", () => {
      throw new Error("gone");
    });
    const { onToast } = renderPanel(daemon);

    await waitFor(() => {
      expect(screen.getByText("laptop")).toBeInTheDocument();
    });
    await user.click(screen.getByRole("button", { name: "Revoke" }));
    await user.click(screen.getByRole("button", { name: "Confirm revoke" }));

    await waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("error", "Revoke failed", "gone");
    });
  });

  it("hides every mutation without the control grant", async () => {
    renderPanel(daemonWith([fx.device()]), { canControl: false });
    await waitFor(() => {
      expect(screen.getByText("laptop")).toBeInTheDocument();
    });
    expect(screen.queryByText("+ New device")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Revoke" })).not.toBeInTheDocument();
  });
});
