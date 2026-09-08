import { afterEach, describe, expect, it, vi } from "vitest";
import { revealBotWorkspace, revealProject } from "./reveal";

const invoke = vi.hoisted(() => vi.fn<(command: string, args: unknown) => Promise<unknown>>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

afterEach(() => {
  vi.unstubAllGlobals();
  invoke.mockReset();
});

describe("revealProject", () => {
  it("does nothing outside the Tauri shell", async () => {
    vi.stubGlobal("window", {});
    await revealProject("acme");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("reveals the project directory through the Tauri command", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValue(null);
    await revealProject("acme");
    expect(invoke).toHaveBeenCalledWith("reveal_project", { dirName: "acme" });
  });

  it("swallows a failure from the shell", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invoke.mockRejectedValue(new Error("no such directory"));
    await expect(revealProject("acme")).resolves.toBeUndefined();
  });
});

describe("revealBotWorkspace", () => {
  it("does nothing outside the Tauri shell", async () => {
    vi.stubGlobal("window", {});
    await revealBotWorkspace("/tmp/acme/bots/alice/workspace");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("reveals the bot workspace through the Tauri command", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValue(null);
    await revealBotWorkspace("/tmp/acme/bots/alice/workspace");
    expect(invoke).toHaveBeenCalledWith("reveal_bot_workspace", {
      workspacePath: "/tmp/acme/bots/alice/workspace",
    });
  });

  it("swallows a failure from the shell", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invoke.mockRejectedValue(new Error("no such workspace"));
    await expect(revealBotWorkspace("/tmp/acme/bots/alice/workspace")).resolves.toBeUndefined();
  });
});
