import { vi } from "vitest";

interface FileDropOptions {
  readonly onDrop: (input: string) => void;
  readonly onError: (error: unknown) => void;
}

interface FileDropDouble {
  onDrop?: (input: string) => void;
  onError?: (error: unknown) => void;
  readonly unlisten: () => void;
  readonly listen: (options: FileDropOptions) => () => void;
  readonly reset: () => void;
}

/** Captures the callbacks TerminalPane hands to the native drag-drop listener. */
export const fileDropDouble: FileDropDouble = {
  unlisten: vi.fn<() => void>(),
  listen: vi.fn<(options: FileDropOptions) => () => void>((options) => {
    fileDropDouble.onDrop = options.onDrop;
    fileDropDouble.onError = options.onError;
    return fileDropDouble.unlisten;
  }),
  reset: () => {
    fileDropDouble.onDrop = undefined;
    fileDropDouble.onError = undefined;
  },
};

export const analyticsDouble = {
  captureException: vi.fn<(error: unknown, operation: string) => void>(),
};

interface TerminalDouble {
  /** Set by the pane's `onData` subscription; call it to simulate typing. */
  onDataHandler?: (data: string) => void;
  /** Set by `attachCustomKeyEventHandler`; call it to simulate a key chord. */
  keyHandler?: (event: KeyboardEvent) => boolean;
  readonly reset: () => void;
}

/**
 * The xterm instance double every TerminalPane test file renders against.
 *
 * `new Terminal()` yields this object: a constructor that returns an object
 * produces that object, so each file's `@xterm/xterm` mock only has to decide
 * what it captures from the options.
 */
export const terminalDouble = {
  cols: 80,
  rows: 24,
  // No `element`, so the pane falls back to its own container.
  element: undefined,
  options: { fontSize: 13, fontFamily: '"SF Mono", "Menlo", "Monaco", monospace' } as {
    fontSize: number;
    fontFamily: string;
  },
  loadAddon: vi.fn<(addon: unknown) => void>(),
  // Real xterm builds the scrollable viewport when it opens; panes read and
  // restore its offset across re-parenting, so the double provides one too.
  open: vi.fn<(container: HTMLElement) => void>((container) => {
    const viewport = document.createElement("div");
    viewport.className = "xterm-viewport";
    container.appendChild(viewport);
  }),
  write: vi.fn<(data: string, done?: () => void) => void>((_data, done) => {
    done?.();
  }),
  reset: vi.fn<() => void>(),
  refresh: vi.fn<(start: number, end: number) => void>(),
  scrollToBottom: vi.fn<() => void>(),
  focus: vi.fn<() => void>(),
  dispose: vi.fn<() => void>(),
  onData: vi.fn<(handler: (data: string) => void) => { dispose: () => void }>((handler) => {
    terminals.onDataHandler = handler;
    return { dispose: vi.fn<() => void>() };
  }),
  attachCustomKeyEventHandler: vi.fn<(handler: (event: KeyboardEvent) => boolean) => void>(
    (handler) => {
      terminals.keyHandler = handler;
    },
  ),
};

/** The captured callbacks, kept beside the double so tests can clear them. */
export const terminals: TerminalDouble = {
  reset: () => {
    terminals.onDataHandler = undefined;
    terminals.keyHandler = undefined;
  },
};
