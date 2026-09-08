import { PhysicalPosition } from "@tauri-apps/api/dpi";
import type { DragDropEvent } from "@tauri-apps/api/window";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { formatDroppedPaths, listenForTerminalFileDrops } from "./terminalFileDrop";

interface DragDropEnvelope {
  readonly payload: DragDropEvent;
}

const mocks = vi.hoisted(() => {
  const state: { dragDropHandler?: (event: DragDropEnvelope) => void } = {};
  const removeListener = vi.fn<() => void>();
  const elementFromPoint = vi.fn<(x: number, y: number) => Element | null>();
  return {
    state,
    isTauri: vi.fn<() => boolean>(),
    scaleFactor: vi.fn<() => Promise<number>>(),
    removeListener,
    elementFromPoint,
    onDragDropEvent: vi.fn<(handler: (event: DragDropEnvelope) => void) => Promise<() => void>>(
      async (handler) => {
        state.dragDropHandler = handler;
        return removeListener;
      },
    ),
  };
});

vi.mock("@tauri-apps/api/core", () => ({ isTauri: mocks.isTauri }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onDragDropEvent: mocks.onDragDropEvent,
    scaleFactor: mocks.scaleFactor,
  }),
}));

function drop(paths: readonly string[], x: number, y: number): void {
  mocks.state.dragDropHandler?.({
    payload: {
      type: "drop",
      paths: [...paths],
      position: new PhysicalPosition(x, y),
    },
  });
}

describe("terminal file drops", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(document, "elementFromPoint", {
      configurable: true,
      value: mocks.elementFromPoint,
    });
    mocks.state.dragDropHandler = undefined;
    mocks.isTauri.mockReturnValue(true);
    mocks.scaleFactor.mockResolvedValue(2);
  });

  it("formats paths as shell-safe terminal input", () => {
    expect(formatDroppedPaths(["/tmp/first image.png", "/tmp/user's second.png"])).toBe(
      "'/tmp/first image.png' '/tmp/user'\\''s second.png' ",
    );
  });

  it("forwards drops over a writable terminal in logical coordinates", async () => {
    const element = document.createElement("div");
    element.getBoundingClientRect = () => new DOMRect(20, 30, 100, 80);
    mocks.elementFromPoint.mockReturnValue(element);
    const onDrop = vi.fn<(input: string) => void>();
    const onError = vi.fn<(error: unknown) => void>();
    const stop = listenForTerminalFileDrops({ element, canDrop: () => true, onDrop, onError });
    await vi.waitFor(() => {
      expect(mocks.state.dragDropHandler).toBeDefined();
    });

    drop(["/tmp/image.png"], 80, 100);
    await vi.waitFor(() => {
      expect(onDrop).toHaveBeenCalledWith("'/tmp/image.png' ");
    });
    expect(onError).not.toHaveBeenCalled();

    stop();
    expect(mocks.removeListener).toHaveBeenCalledOnce();
  });

  it("ignores drops when the terminal cannot receive them", async () => {
    const element = document.createElement("div");
    document.body.appendChild(element);
    element.getBoundingClientRect = () => new DOMRect(20, 30, 100, 80);
    mocks.elementFromPoint.mockReturnValue(element);
    let writable = false;
    const onDrop = vi.fn<(input: string) => void>();
    listenForTerminalFileDrops({
      element,
      canDrop: () => writable,
      onDrop,
      onError: vi.fn<(error: unknown) => void>(),
    });
    await vi.waitFor(() => {
      expect(mocks.state.dragDropHandler).toBeDefined();
    });

    drop(["/tmp/read-only.png"], 80, 100);
    writable = true;
    drop([], 80, 100);
    drop(["/tmp/outside.png"], 10, 10);
    element.style.visibility = "hidden";
    drop(["/tmp/hidden.png"], 80, 100);
    await Promise.resolve();
    expect(onDrop).not.toHaveBeenCalled();
  });

  it("uses the current display scale for each drop", async () => {
    const element = document.createElement("div");
    element.getBoundingClientRect = () => new DOMRect(100, 100, 100, 100);
    mocks.elementFromPoint.mockReturnValue(element);
    mocks.scaleFactor.mockResolvedValueOnce(2).mockResolvedValueOnce(1);
    const onDrop = vi.fn<(input: string) => void>();
    listenForTerminalFileDrops({
      element,
      canDrop: () => true,
      onDrop,
      onError: vi.fn<(error: unknown) => void>(),
    });
    await vi.waitFor(() => {
      expect(mocks.state.dragDropHandler).toBeDefined();
    });

    drop(["/tmp/retina.png"], 300, 300);
    drop(["/tmp/standard.png"], 150, 150);

    await vi.waitFor(() => {
      expect(onDrop).toHaveBeenCalledTimes(2);
    });
  });

  it("ignores drops when another element covers the terminal", async () => {
    const element = document.createElement("div");
    element.getBoundingClientRect = () => new DOMRect(20, 30, 100, 80);
    mocks.elementFromPoint.mockReturnValue(document.body);
    const onDrop = vi.fn<(input: string) => void>();
    listenForTerminalFileDrops({
      element,
      canDrop: () => true,
      onDrop,
      onError: vi.fn<(error: unknown) => void>(),
    });
    await vi.waitFor(() => {
      expect(mocks.state.dragDropHandler).toBeDefined();
    });

    drop(["/tmp/covered.png"], 80, 100);
    await Promise.resolve();

    expect(onDrop).not.toHaveBeenCalled();
  });

  it("does not register a native listener in browser development", () => {
    mocks.isTauri.mockReturnValue(false);
    const element = document.createElement("div");
    const stop = listenForTerminalFileDrops({
      element,
      canDrop: () => true,
      onDrop: vi.fn<(input: string) => void>(),
      onError: vi.fn<(error: unknown) => void>(),
    });

    expect(mocks.scaleFactor).not.toHaveBeenCalled();
    expect(mocks.onDragDropEvent).not.toHaveBeenCalled();
    stop();
  });

  it("removes a listener that finishes registering after disposal", async () => {
    let resolveListener: ((value: () => void) => void) | undefined;
    mocks.onDragDropEvent.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveListener = resolve;
      }),
    );
    const stop = listenForTerminalFileDrops({
      element: document.createElement("div"),
      canDrop: () => true,
      onDrop: vi.fn<(input: string) => void>(),
      onError: vi.fn<(error: unknown) => void>(),
    });
    stop();
    resolveListener?.(mocks.removeListener);
    await vi.waitFor(() => {
      expect(mocks.removeListener).toHaveBeenCalledOnce();
    });

    expect(mocks.scaleFactor).not.toHaveBeenCalled();
  });

  it("reports native listener and scale lookup failures", async () => {
    const registrationError = new Error("listener failed");
    const onRegistrationError = vi.fn<(error: unknown) => void>();
    mocks.onDragDropEvent.mockRejectedValueOnce(registrationError);
    listenForTerminalFileDrops({
      element: document.createElement("div"),
      canDrop: () => true,
      onDrop: vi.fn<(input: string) => void>(),
      onError: onRegistrationError,
    });
    await vi.waitFor(() => {
      expect(onRegistrationError).toHaveBeenCalledWith(registrationError);
    });

    const scaleError = new Error("scale failed");
    const onScaleError = vi.fn<(error: unknown) => void>();
    mocks.scaleFactor.mockRejectedValueOnce(scaleError);
    listenForTerminalFileDrops({
      element: document.createElement("div"),
      canDrop: () => true,
      onDrop: vi.fn<(input: string) => void>(),
      onError: onScaleError,
    });
    await vi.waitFor(() => {
      expect(mocks.state.dragDropHandler).toBeDefined();
    });
    drop(["/tmp/image.png"], 80, 100);

    await vi.waitFor(() => {
      expect(onScaleError).toHaveBeenCalledWith(scaleError);
    });
  });
});
