import { beforeEach, describe, expect, it, vi } from "vitest";

const plugin = vi.hoisted(() => ({
  isPermissionGranted: vi.fn<() => Promise<boolean>>(),
  requestPermission: vi.fn<() => Promise<string>>(),
  sendNotification: vi.fn<(options: { title: string; body: string }) => void>(),
}));
vi.mock("@tauri-apps/plugin-notification", () => plugin);

const isTauri = vi.hoisted(() => vi.fn<() => boolean>());
vi.mock("./tauri", () => ({ isTauri }));

const { notifyNatively } = await import("./notify");

beforeEach(() => {
  plugin.isPermissionGranted.mockReset();
  plugin.requestPermission.mockReset();
  isTauri.mockReturnValue(true);
  plugin.isPermissionGranted.mockResolvedValue(true);
  plugin.requestPermission.mockResolvedValue("granted");
  plugin.sendNotification.mockReset();
});

describe("notifyNatively", () => {
  it("raises the notice when permission is already held", async () => {
    await notifyNatively("A decision is due", "Waive rule 3?");
    expect(plugin.sendNotification).toHaveBeenCalledWith({
      title: "A decision is due",
      body: "Waive rule 3?",
    });
    expect(plugin.requestPermission).not.toHaveBeenCalled();
  });

  it("asks once when permission has not been given yet", async () => {
    plugin.isPermissionGranted.mockResolvedValue(false);
    await notifyNatively("A decision is due", "Waive rule 3?");
    expect(plugin.requestPermission).toHaveBeenCalled();
    expect(plugin.sendNotification).toHaveBeenCalled();
  });

  // The sidebar badge still carries the same count, so a refusal is not worth
  // a second prompt or an error the owner has to deal with.
  it("stays quiet when the user declined", async () => {
    plugin.isPermissionGranted.mockResolvedValue(false);
    plugin.requestPermission.mockResolvedValue("denied");
    await notifyNatively("A decision is due", "Waive rule 3?");
    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });

  it("does nothing outside the Tauri shell", async () => {
    isTauri.mockReturnValue(false);
    await notifyNatively("A decision is due", "Waive rule 3?");
    expect(plugin.isPermissionGranted).not.toHaveBeenCalled();
    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });

  it("swallows a shell failure rather than surfacing it", async () => {
    plugin.isPermissionGranted.mockRejectedValue(new Error("no notification centre"));
    await expect(notifyNatively("A decision is due", "Waive rule 3?")).resolves.toBeUndefined();
  });
});
