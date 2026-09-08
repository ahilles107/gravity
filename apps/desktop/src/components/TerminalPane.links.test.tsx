import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AddToast } from "../app/useToasts";
import { FakeDaemon } from "../test/fakeDaemon";
import TerminalPane from "./TerminalPane";
import { clearTerminalCache } from "./terminalCache";

type LinkCallback = (event: MouseEvent, text: string) => void;
type TerminalOptions = {
  readonly linkHandler?: {
    readonly activate: LinkCallback;
    readonly hover?: LinkCallback;
    readonly leave?: LinkCallback;
  };
};

const openExternalUrl = vi.hoisted(() => vi.fn<(url: string) => Promise<void>>());
vi.mock("../externalUrl", () => ({ openExternalUrl }));

const onToast = vi.fn<AddToast>();

let webLinkHandler: LinkCallback | null = null;
let terminalOptions: TerminalOptions | null = null;

vi.mock("@xterm/xterm", async () => {
  const { terminalDouble } = await import("../test/terminalPaneDoubles");
  return {
    Terminal: function TerminalDouble(options: TerminalOptions): typeof terminalDouble {
      terminalOptions = options;
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
  WebLinksAddon: function WebLinksAddonDouble(handler: LinkCallback): object {
    webLinkHandler = handler;
    return {};
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

function pane(): ReturnType<typeof render> {
  return render(<TerminalPane client={daemon()} botId="b1" canWrite={false} onToast={onToast} />);
}

describe("TerminalPane links", () => {
  beforeEach(() => {
    clearTerminalCache();
    vi.clearAllMocks();
    webLinkHandler = null;
    terminalOptions = null;
    openExternalUrl.mockResolvedValue();
  });

  it("opens plain and OSC 8 web links outside the terminal", async () => {
    pane();
    await vi.waitFor(() => {
      expect(webLinkHandler).not.toBeNull();
      expect(terminalOptions?.linkHandler).toBeDefined();
    });

    const event = new MouseEvent("click");
    webLinkHandler?.(event, "https://example.com/plain");
    terminalOptions?.linkHandler?.activate(event, "https://example.com/osc-8");

    expect(openExternalUrl).toHaveBeenNthCalledWith(1, "https://example.com/plain");
    expect(openExternalUrl).toHaveBeenNthCalledWith(2, "https://example.com/osc-8");
  });

  it("shows where a hovered OSC 8 link really goes, then clears it on leave", async () => {
    const view = pane();
    await vi.waitFor(() => {
      expect(terminalOptions?.linkHandler?.hover).toBeDefined();
    });

    const event = new MouseEvent("mousemove", { clientX: 30, clientY: 40 });
    terminalOptions?.linkHandler?.hover?.(event, "https://evil.example/login");

    expect(view.container.querySelector(".terminal-link-target")?.textContent).toBe(
      "https://evil.example/login",
    );

    terminalOptions?.linkHandler?.leave?.(event, "https://evil.example/login");
    expect(view.container.querySelector(".terminal-link-target")).toBeNull();
  });

  it("replaces the tooltip when the pointer moves to another link", async () => {
    const view = pane();
    await vi.waitFor(() => {
      expect(terminalOptions?.linkHandler?.hover).toBeDefined();
    });

    const event = new MouseEvent("mousemove");
    terminalOptions?.linkHandler?.hover?.(event, "https://one.example");
    terminalOptions?.linkHandler?.hover?.(event, "https://two.example");

    const tooltips = view.container.querySelectorAll(".terminal-link-target");
    expect(tooltips).toHaveLength(1);
    expect(tooltips[0]?.textContent).toBe("https://two.example");
  });

  it("reports a link that fails to open", async () => {
    openExternalUrl.mockRejectedValue(new Error("no browser"));
    pane();
    await vi.waitFor(() => {
      expect(webLinkHandler).not.toBeNull();
    });

    webLinkHandler?.(new MouseEvent("click"), "https://example.com/plain");

    await vi.waitFor(() => {
      expect(onToast).toHaveBeenCalledWith("warn", "Could not open link", "no browser");
    });
  });
});
