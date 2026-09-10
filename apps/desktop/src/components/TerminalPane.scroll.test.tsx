import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FakeDaemon } from "../test/fakeDaemon";
import { terminals } from "../test/terminalPaneDoubles";
import TerminalPane from "./TerminalPane";
import { clearTerminalCache } from "./terminalCache";

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
    onContextLoss = vi.fn<(handler: () => void) => void>();
    dispose = vi.fn<() => void>();
  },
}));
vi.mock("@xterm/addon-web-links", () => ({
  WebLinksAddon: class {
    dispose = vi.fn<() => void>();
  },
}));

vi.stubGlobal(
  "ResizeObserver",
  class {
    observe = vi.fn<(target: Element) => void>();
    disconnect = vi.fn<() => void>();
  },
);

function daemon(): FakeDaemon {
  return new FakeDaemon().onRequest("detach", () => ({ type: "ok", req_id: "1" }));
}

function viewport(): HTMLElement {
  const element = document.querySelector(".xterm-viewport");
  if (!(element instanceof HTMLElement)) {
    throw new Error("terminal viewport missing");
  }
  return element;
}

describe("TerminalPane scroll position", () => {
  beforeEach(() => {
    clearTerminalCache();
    vi.clearAllMocks();
    terminals.reset();
  });

  it("restores the offset a re-parented terminal loses on detach", async () => {
    const fake = daemon();
    const first = render(<TerminalPane client={fake} botId="b1" canWrite />);
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false]);
    });
    const kept = viewport();
    kept.scrollTop = 4200;
    kept.dispatchEvent(new Event("scroll"));
    first.unmount();
    // What the browser does to a scrollable element that leaves the document.
    kept.scrollTop = 0;

    fake.attachResult = { seq: 9, resumed: true };
    render(<TerminalPane client={fake} botId="b1" canWrite />);
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false, true]);
    });

    expect(viewport().scrollTop).toBe(4200);
  });

  it("ignores the scroll to the top that comes with detaching", async () => {
    const fake = daemon();
    const first = render(<TerminalPane client={fake} botId="b1" canWrite />);
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false]);
    });
    const kept = viewport();
    kept.scrollTop = 900;
    kept.dispatchEvent(new Event("scroll"));
    first.unmount();
    // The parked terminal is out of the document; the browser scrolls it to
    // the top and the event that follows must not overwrite what was kept.
    kept.scrollTop = 0;
    kept.dispatchEvent(new Event("scroll"));

    fake.attachResult = { seq: 9, resumed: true };
    render(<TerminalPane client={fake} botId="b1" canWrite />);
    await vi.waitFor(() => {
      expect(fake.attachResumes).toEqual([false, true]);
    });

    expect(viewport().scrollTop).toBe(900);
  });
});
