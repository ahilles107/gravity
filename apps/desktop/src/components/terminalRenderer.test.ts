import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadWebgl } from "./terminalRenderer";

let constructionError: Error | null = null;
let contextLossHandler: (() => void) | null = null;
let constructed = 0;

vi.mock("@xterm/addon-webgl", () => ({
  WebglAddon: class {
    constructor() {
      constructed += 1;
      if (constructionError !== null) {
        throw constructionError;
      }
    }
    onContextLoss = (handler: () => void): void => {
      contextLossHandler = handler;
    };
    dispose = vi.fn<() => void>();
  },
}));

describe("loadWebgl", () => {
  const term = { loadAddon: vi.fn<(addon: unknown) => void>() };
  let element: HTMLElement;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    constructionError = null;
    contextLossHandler = null;
    constructed = 0;
    element = document.createElement("div");
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("records the DOM fallback when no WebGL2 context is available", () => {
    constructionError = new Error("WebGL2 unavailable");
    loadWebgl(term, element);
    expect(element.dataset.renderer).toBe("dom");
    expect(element.dataset.rendererFallback).toBe("WebGL2 unavailable");
    expect(term.loadAddon).not.toHaveBeenCalled();
  });

  it("reacquires WebGL after a lost context instead of staying on the DOM renderer", () => {
    loadWebgl(term, element);
    expect(element.dataset.renderer).toBe("webgl");

    contextLossHandler?.();
    expect(element.dataset.renderer).toBe("dom");
    expect(element.dataset.rendererFallback).toBe("WebGL context lost");

    vi.advanceTimersByTime(999);
    expect(element.dataset.renderer).toBe("dom");
    vi.advanceTimersByTime(1);
    expect(element.dataset.renderer).toBe("webgl");
    expect(element.dataset.rendererFallback).toBeUndefined();
    expect(term.loadAddon).toHaveBeenCalledTimes(2);
  });

  it("backs off between losses and gives up after the last delay", () => {
    loadWebgl(term, element);
    contextLossHandler?.();
    vi.advanceTimersByTime(1_000);
    contextLossHandler?.();
    vi.advanceTimersByTime(1_999);
    expect(element.dataset.renderer).toBe("dom");
    vi.advanceTimersByTime(1);
    expect(element.dataset.renderer).toBe("webgl");
    contextLossHandler?.();
    vi.advanceTimersByTime(4_000);
    expect(element.dataset.renderer).toBe("webgl");
    expect(constructed).toBe(4);

    // Four losses in quick succession is a GPU that will not keep a context:
    // stop asking rather than thrash.
    contextLossHandler?.();
    vi.advanceTimersByTime(60_000);
    expect(element.dataset.renderer).toBe("dom");
    expect(constructed).toBe(4);
  });

  it("starts the retries over once a context has held for a while", () => {
    loadWebgl(term, element);
    for (const delay of [1_000, 2_000, 4_000]) {
      contextLossHandler?.();
      vi.advanceTimersByTime(delay);
    }
    expect(constructed).toBe(4);

    vi.advanceTimersByTime(60_000);
    contextLossHandler?.();
    vi.advanceTimersByTime(1_000);
    expect(element.dataset.renderer).toBe("webgl");
    expect(constructed).toBe(5);
  });
});
