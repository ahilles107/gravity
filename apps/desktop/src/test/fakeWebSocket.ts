import { vi } from "vitest";

type Listener = (event: Event) => void;

/**
 * The browser `WebSocket` surface `DaemonClient` drives, plus the methods a
 * test uses to script the server side. Every member is invoked dynamically —
 * through the global `WebSocket` or through `SocketHarness` — so the interface
 * is what `usedClassMembers` in `fallow.config.json` exempts.
 */
interface ScriptedWebSocket {
  readyState: number;
  addEventListener(type: string, listener: Listener): void;
  send(data: string): void;
  close(): void;
  open(): void;
  receive(payload: unknown): void;
  receiveRaw(data: unknown): void;
  reqId(index: number): string;
}

/** Minimal scripted WebSocket, enough for `DaemonClient` to drive. */
export class FakeSocket implements ScriptedWebSocket {
  static readonly OPEN = 1;
  static readonly CLOSED = 3;

  readonly url: string;
  readonly sent: string[] = [];
  readyState: number = FakeSocket.OPEN;
  closed = false;

  private readonly listeners = new Map<string, Set<Listener>>();

  constructor(url: string) {
    this.url = url;
  }

  addEventListener(type: string, listener: Listener): void {
    const set = this.listeners.get(type) ?? new Set();
    this.listeners.set(type, set);
    set.add(listener);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.closed = true;
    this.readyState = FakeSocket.CLOSED;
    this.dispatch("close", new Event("close"));
  }

  open(): void {
    this.dispatch("open", new Event("open"));
  }

  /** Delivers a server frame to the client. */
  receive(payload: unknown): void {
    this.dispatch("message", new MessageEvent("message", { data: JSON.stringify(payload) }));
  }

  receiveRaw(data: unknown): void {
    this.dispatch("message", new MessageEvent("message", { data }));
  }

  /** The `req_id` of the nth frame this socket was asked to send. */
  reqId(index: number): string {
    const frame: unknown = JSON.parse(this.sent[index] ?? "{}");
    if (typeof frame === "object" && frame !== null && "req_id" in frame) {
      const value: unknown = Reflect.get(frame, "req_id");
      if (typeof value === "string") {
        return value;
      }
    }
    throw new Error(`frame ${index} has no req_id`);
  }

  private dispatch(type: string, event: Event): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }
}

export interface SocketHarness {
  /** Every socket the client has constructed, oldest first. */
  readonly sockets: readonly FakeSocket[];
  latest: () => FakeSocket;
}

/** Installs `FakeSocket` as the global `WebSocket` for the current test. */
export function installFakeWebSocket(): SocketHarness {
  const sockets: FakeSocket[] = [];
  vi.stubGlobal(
    "WebSocket",
    Object.assign(
      class extends FakeSocket {
        constructor(url: string) {
          super(url);
          sockets.push(this);
        }
      },
      { OPEN: FakeSocket.OPEN, CLOSED: FakeSocket.CLOSED },
    ),
  );
  return {
    sockets,
    latest: () => {
      const socket = sockets[sockets.length - 1];
      if (socket === undefined) {
        throw new Error("no socket was created");
      }
      return socket;
    },
  };
}
