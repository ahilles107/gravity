import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AddToast } from "../app/useToasts";
import { FakeDaemon } from "../test/fakeDaemon";
import {
  analyticsDouble,
  fileDropDouble,
  terminalDouble as term,
  terminals,
} from "../test/terminalPaneDoubles";
import TerminalPane from "./TerminalPane";
import { clearTerminalCache } from "./terminalCache";

// Link behaviour lives in `TerminalPane.links.test.tsx`.
const onToast = vi.fn<AddToast>();

let resizeObserved: (() => void) | null = null;
let webglConstructionError: Error | null = null;
let webglContextLossHandler: (() => void) | null = null;

vi.mock("@xterm/xterm", async () => {
  const { terminalDouble } = await import("../test/terminalPaneDoubles");
  return {
    Terminal: function TerminalDouble(): typeof terminalDouble {
      return terminalDouble;
    },
  };
});
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit = vi.fn<() => void>();
  },
}));
vi.mock("@xterm/addon-webgl", () => ({
  WebglAddon: class {
    constructor() {
      if (webglConstructionError !== null) {
        throw webglConstructionError;
      }
    }
    onContextLoss = vi.fn<(handler: () => void) => void>((handler) => {
      webglContextLossHandler = handler;
    });
    dispose = vi.fn<() => void>();
  },
}));
vi.mock("@xterm/addon-web-links", () => ({
  WebLinksAddon: class {
    dispose = vi.fn<() => void>();
  },
}));
vi.mock("../analytics", async () => (await import("../test/terminalPaneDoubles")).analyticsDouble);
vi.mock("./terminalFileDrop", async () => ({
  listenForTerminalFileDrops: (await import("../test/terminalPaneDoubles")).fileDropDouble.listen,
}));

vi.stubGlobal(
  "ResizeObserver",
  class {
    constructor(callback: () => void) {
      resizeObserved = callback;
    }
    observe = vi.fn<(target: Element) => void>();
    disconnect = vi.fn<() => void>();
  },
);

function daemon(): FakeDaemon {
  return new FakeDaemon().onRequest("detach", () => ({ type: "ok", req_id: "1" }));
}

describe("TerminalPane", () => {
  beforeEach(() => {
    // Emptying the cache disposes leftovers, so it must precede the mock reset.
    clearTerminalCache();
    vi.clearAllMocks();
    terminals.reset();
    resizeObserved = null;
    webglConstructionError = null;
    webglContextLossHandler = null;
    fileDropDouble.reset();
  });

  it("attaches, writes pushed output and detaches on unmount", async () => {
    const fake = daemon();
    const view = render(
      <TerminalPane client={fake} botId="b1" canWrite={false} onToast={onToast} />,
    );
    await vi.waitFor(() => {
      expect(term.open).toHaveBeenCalled();
    });

    fake.emit("term", { type: "term", bot_id: "b1", seq: 1, data: "hello" });
    expect(term.write).toHaveBeenCalledWith("hello");

    fake.emit("term", { type: "term", bot_id: "other", seq: 1, data: "ignored" });
    expect(term.write).toHaveBeenCalledTimes(1);

    view.unmount();
    expect(fake.requests.some((r) => r.body.type === "detach")).toBe(true);
    // The instance is parked for reuse, not disposed with the pane.
    expect(term.dispose).not.toHaveBeenCalled();
  });

  it("sends ESC+CR for Shift+Enter so the prompt gets a newline", async () => {
    const fake = daemon();
    render(<TerminalPane client={fake} botId="b1" canWrite onToast={onToast} />);
    await vi.waitFor(() => {
      expect(terminals.keyHandler).toBeDefined();
    });

    const shiftEnter = new KeyboardEvent("keydown", {
      key: "Enter",
      shiftKey: true,
      cancelable: true,
    });
    expect(terminals.keyHandler?.(shiftEnter)).toBe(false);
    expect(shiftEnter.defaultPrevented).toBe(true);
    expect(fake.fired).toContainEqual({ type: "input", bot_id: "b1", data: "\u001b\r" });

    // A plain Enter still submits: xterm handles it as usual.
    const plainEnter = new KeyboardEvent("keydown", { key: "Enter" });
    expect(terminals.keyHandler?.(plainEnter)).toBe(true);
    expect(fake.fired.filter((f) => f.type === "input")).toHaveLength(1);
  });

  it("forwards dropped file paths and focuses the terminal", async () => {
    const fake = daemon();
    const view = render(<TerminalPane client={fake} botId="b1" canWrite />);
    await vi.waitFor(() => {
      expect(fileDropDouble.onDrop).toBeDefined();
    });

    fileDropDouble.onDrop?.("'/tmp/image.png' ");
    expect(fake.fired).toContainEqual({
      type: "input",
      bot_id: "b1",
      data: "'/tmp/image.png' ",
    });
    expect(term.focus).toHaveBeenCalled();

    view.unmount();
    expect(fileDropDouble.unlisten).toHaveBeenCalled();
  });

  it("reports file drop listener failures", async () => {
    render(<TerminalPane client={daemon()} botId="b1" canWrite onToast={onToast} />);
    await vi.waitFor(() => {
      expect(fileDropDouble.onError).toBeDefined();
    });

    const error = new Error("native listener failed");
    fileDropDouble.onError?.(error);

    expect(analyticsDouble.captureException).toHaveBeenCalledWith(error, "terminal_file_drop");
    expect(onToast).toHaveBeenCalledWith("error", "File drop unavailable", error.message);
  });

  it("does not send terminal input while a modal dialog is open", async () => {
    const fake = daemon();
    render(<TerminalPane client={fake} botId="b1" canWrite />);
    await vi.waitFor(() => {
      expect(terminals.onDataHandler).toBeDefined();
    });
    const modal = document.createElement("div");
    modal.setAttribute("aria-modal", "true");
    document.body.appendChild(modal);

    terminals.onDataHandler?.("blocked");

    expect(fake.fired.filter((frame) => frame.type === "input")).toHaveLength(0);
    modal.remove();
  });

  it("reuses a cached terminal and resumes instead of replaying", async () => {
    const fake = daemon();
    const first = render(<TerminalPane client={fake} botId="b1" canWrite onToast={onToast} />);
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false]);
    });
    first.unmount();

    fake.attachResult = { seq: 9, resumed: true };
    fake.fired.length = 0;
    // The first mount's full replay legitimately reset; the reuse must not.
    term.reset.mockClear();
    render(<TerminalPane client={fake} botId="b1" canWrite onToast={onToast} />);
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false, true]);
    });
    expect(term.open).toHaveBeenCalledTimes(1);
    expect(term.refresh).toHaveBeenCalled();
    expect(term.reset).not.toHaveBeenCalled();
    // A resumed replay is contiguous with the kept screen: no forced repaint.
    expect(fake.fired.every((f) => f.type !== "resize" || f.force === false)).toBe(true);
  });

  it("discards cached terminal data when the daemon endpoint changes", async () => {
    const fake = daemon();
    const first = render(
      <TerminalPane client={fake} botId="b1" canWrite={false} onToast={onToast} />,
    );
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false]);
    });
    first.unmount();

    fake.setEndpoint({ host: "other", port: 8888 });
    render(<TerminalPane client={fake} botId="b1" canWrite={false} onToast={onToast} />);
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false, false]);
    });
    expect(term.dispose).toHaveBeenCalledTimes(1);
    expect(term.open).toHaveBeenCalledTimes(2);
  });

  it("exposes WebGL fallback and context loss for diagnostics", async () => {
    webglConstructionError = new Error("WebGL2 unavailable");
    const fake = daemon();
    const failed = render(
      <TerminalPane client={fake} botId="b1" canWrite={false} onToast={onToast} />,
    );
    await vi.waitFor(() => {
      expect(failed.container.querySelector("[data-renderer='dom']")).not.toBeNull();
    });
    expect(
      failed.container.querySelector("[data-renderer-fallback='WebGL2 unavailable']"),
    ).not.toBeNull();
    failed.unmount();

    webglConstructionError = null;
    const recovered = render(
      <TerminalPane client={fake} botId="b2" canWrite={false} onToast={onToast} />,
    );
    await vi.waitFor(() => {
      expect(webglContextLossHandler).not.toBeNull();
    });
    webglContextLossHandler?.();
    expect(
      recovered.container.querySelector("[data-renderer-fallback='WebGL context lost']"),
    ).not.toBeNull();
  });

  it("evicts the least recently viewed terminal beyond the cache capacity", async () => {
    const fake = daemon();
    // xterm's WebGL teardown can throw once a context is gone; eviction of a
    // parked terminal must swallow that rather than crash the app.
    term.dispose.mockImplementationOnce(() => {
      throw new Error("renderer already lost its context");
    });
    const visit = async (botId: string, nthAttach: number): Promise<void> => {
      const view = render(
        <TerminalPane client={fake} botId={botId} canWrite={false} onToast={onToast} />,
      );
      await vi.waitFor(() => {
        expect(fake.attachResumes).toHaveLength(nthAttach);
      });
      view.unmount();
    };
    await visit("b1", 1);
    await visit("b2", 2);
    await visit("b3", 3);
    await visit("b4", 4);
    // Three slots: parking b4 pushed b1 out, and only b1.
    expect(term.dispose).toHaveBeenCalledTimes(1);
  });

  it("resets the terminal when the replay cursor cannot be resumed", async () => {
    const fake = daemon();
    fake.attachResult = { seq: 0, resumed: false };
    render(<TerminalPane client={fake} botId="b1" canWrite={false} onToast={onToast} />);
    await vi.waitFor(() => {
      expect(term.reset).toHaveBeenCalled();
    });
  });

  it("keeps the replayed screen and lets the runtime repaint over it", async () => {
    const fake = daemon();
    fake.attachResult = { seq: 2, resumed: false };
    render(<TerminalPane client={fake} botId="b1" canWrite onToast={onToast} />);
    await vi.waitFor(() => {
      expect(term.reset).toHaveBeenCalled();
    });

    // The server trims a mid-session replay to a line boundary, so every frame
    // it sends belongs on the screen.
    fake.emit("term", { type: "term", bot_id: "b1", seq: 1, data: "a whole line" });
    fake.emit("term", { type: "term", bot_id: "b1", seq: 2, data: "and the rest" });
    fake.emit("term", { type: "term", bot_id: "b1", seq: 3, data: "repaint" });
    expect(term.write.mock.calls.map(([data]) => data)).toEqual([
      "a whole line",
      "and the rest",
      "repaint",
    ]);
    // A bulk replay can unpin xterm's viewport, so the final replay write
    // follows to the bottom once; live frames rely on xterm's own behaviour.
    expect(term.scrollToBottom).toHaveBeenCalledTimes(1);
  });

  it("leaves the runtime alone when an attach lands after the pane is gone", async () => {
    const fake = daemon();
    fake.deferAttach = true;
    const view = render(<TerminalPane client={fake} botId="b1" canWrite onToast={onToast} />);
    await vi.waitFor(() => {
      expect(fake.attachResumes).toHaveLength(1);
    });

    // Switching bots disposes this terminal mid-attach: the late reply must not
    // touch it, nor make the runtime repaint for a pane that is gone.
    view.unmount();
    fake.fired.length = 0;
    fake.releaseAttaches();
    await Promise.resolve();
    expect(term.reset).not.toHaveBeenCalled();
    expect(fake.fired).toHaveLength(0);
  });

  it("never resumes into the fresh terminal it just created", async () => {
    const fake = daemon();
    fake.attachResult = { seq: 7, resumed: true };
    render(<TerminalPane client={fake} botId="b1" canWrite onToast={onToast} />);
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false]);
    });

    // A reconnect keeps the same terminal, so that attach may resume.
    fake.setStatus("connected");
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false, true]);
    });
  });

  it("forces a repaint on the resize that follows an attach", async () => {
    const fake = daemon();
    render(<TerminalPane client={fake} botId="b1" canWrite onToast={onToast} />);
    await vi.waitFor(() => {
      expect(fake.fired.some((f) => f.type === "resize" && f.force === true)).toBe(true);
    });

    // A plain container resize must not force a repaint.
    fake.fired.length = 0;
    resizeObserved?.();
    expect(fake.fired.every((f) => f.type !== "resize" || f.force === false)).toBe(true);
  });

  it("only forwards input and resize while writable", async () => {
    const fake = daemon();
    const view = render(
      <TerminalPane client={fake} botId="b1" canWrite={false} onToast={onToast} />,
    );
    await vi.waitFor(() => {
      expect(terminals.onDataHandler).toBeDefined();
    });

    terminals.onDataHandler?.("x");
    expect(fake.fired).toHaveLength(0);

    view.rerender(<TerminalPane client={fake} botId="b1" canWrite onToast={onToast} />);
    await vi.waitFor(() => {
      expect(fake.fired.some((frame) => frame.type === "resize")).toBe(true);
    });
    expect(term.focus).toHaveBeenCalled();

    terminals.onDataHandler?.("y");
    expect(fake.fired.some((frame) => frame.type === "input")).toBe(true);
  });

  it("re-attaches when the connection comes back and refits on resize", async () => {
    const fake = daemon();
    render(<TerminalPane client={fake} botId="b1" canWrite={false} onToast={onToast} />);
    await vi.waitFor(() => {
      expect(resizeObserved).not.toBeNull();
    });

    resizeObserved?.();
    fake.setStatus("connected");
    await vi.waitFor(() => {
      expect(term.open).toHaveBeenCalled();
    });
  });
});
