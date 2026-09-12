import type { AttachResult, DaemonApi } from "./api";
import { CLIENT_ID, DaemonError, PROTOCOL_VERSION } from "./connection";
import type { ConnectionStatus, Endpoint } from "./connection";
import type { Grant } from "./entities";
import type {
  PushOf,
  ReplyOf,
  ServerPush,
  ServerPushType,
  ServerReply,
  ServerReplyType,
} from "./messages";
import type { PushHandlerSets } from "./push";
import { dispatchPush, emptyHandlers } from "./push";
import type { ClientRequestBody, FireBody, RequestBody } from "./requests";
import { isReply, parseServerMessage, replyIs } from "./wire";

interface PendingRequest {
  readonly resolve: (reply: ServerReply) => void;
  readonly reject: (error: Error) => void;
}

const MIN_BACKOFF_MS = 500;
const MAX_BACKOFF_MS = 15_000;

/**
 * Handshake rejections a retry cannot clear, mapped to the status that says
 * what has to change instead. The app keeps reconnecting so it heals the
 * moment compatible credentials or versions are installed.
 */
const HANDSHAKE_VERDICTS: Readonly<Record<string, ConnectionStatus>> = {
  auth_failed: "auth_failed",
  unsupported_version: "version_mismatch",
};

function isHandshakeVerdict(status: ConnectionStatus): boolean {
  return Object.values(HANDSHAKE_VERDICTS).includes(status);
}

/**
 * WebSocket client for the gravityd control plane.
 *
 * - `hello` handshake with token + protocol version
 * - request/response correlation by `req_id`
 * - typed push subscription via `on(...)`
 * - automatic reconnect with exponential backoff
 * - per-bot `after_seq` cursor tracking for terminal replay
 */
export class DaemonClient implements DaemonApi {
  private endpoint: Endpoint;
  private readonly getToken: () => Promise<string>;
  private ws: WebSocket | null = null;
  private nextReqId = 1;
  private running = false;
  private backoffMs = MIN_BACKOFF_MS;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly pending = new Map<string, PendingRequest>();
  private readonly cursors = new Map<string, number>();
  private readonly statusListeners = new Set<(status: ConnectionStatus) => void>();
  private readonly handlers: PushHandlerSets = emptyHandlers();

  connectionGeneration = 0;
  status: ConnectionStatus = "disconnected";
  capabilities: readonly string[] = [];
  serverVersion = "";
  /** Grants held by this connection (populated during the handshake). */
  grants: readonly Grant[] = [];
  /** Device id when authenticated with a device token, null for the owner token. */
  deviceId: string | null = null;

  constructor(endpoint: Endpoint, getToken: () => Promise<string>) {
    this.endpoint = endpoint;
    this.getToken = getToken;
  }

  start(): void {
    if (this.running) {
      return;
    }
    this.running = true;
    this.connect();
  }

  /** Changes the daemon endpoint and reconnects. */
  setEndpoint(endpoint: Endpoint): void {
    this.endpoint = endpoint;
    this.connectionGeneration += 1;
    this.backoffMs = MIN_BACKOFF_MS;
    if (!this.running) {
      return;
    }
    if (isHandshakeVerdict(this.status)) {
      this.setStatus("disconnected");
    }
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    const ws = this.ws;
    if (ws === null) {
      this.connect();
    } else {
      // The close listener schedules the reconnect against the new endpoint.
      ws.close();
    }
  }

  getEndpoint(): Endpoint {
    return this.endpoint;
  }

  /** Whether the current connection holds the given grant. */
  hasGrant(grant: Grant): boolean {
    return this.grants.includes(grant);
  }

  onStatus(listener: (status: ConnectionStatus) => void): () => void {
    this.statusListeners.add(listener);
    return () => {
      this.statusListeners.delete(listener);
    };
  }

  /** Subscribes to a server push type; returns an unsubscribe function. */
  on<K extends ServerPushType>(type: K, handler: (push: PushOf<K>) => void): () => void {
    const set = this.handlers[type];
    set.add(handler);
    return () => {
      set.delete(handler);
    };
  }

  /**
   * Sends a request and resolves with the reply of the expected type.
   * `error` replies reject with a `DaemonError`.
   */
  async request<K extends ServerReplyType>(body: RequestBody, expect: K): Promise<ReplyOf<K>> {
    const reply = await this.sendRequest(body);
    if (replyIs(reply, "error")) {
      throw new DaemonError(reply.code, reply.message);
    }
    if (replyIs(reply, expect)) {
      return reply;
    }
    throw new DaemonError("protocol_error", `expected ${expect} reply, got ${reply.type}`);
  }

  /** Fire-and-forget frames (`input`, `resize`); silently dropped when disconnected. */
  fire(body: FireBody): void {
    const ws = this.ws;
    if (ws === null || ws.readyState !== WebSocket.OPEN || this.status !== "connected") {
      return;
    }
    ws.send(JSON.stringify({ ...body, req_id: this.newReqId() }));
  }

  /**
   * Attaches to a bot terminal, resuming from the last seen sequence number
   * when the caller still has the matching screen (`resume`). Callers with a
   * fresh terminal must pass false so the server replays the whole ring —
   * the cursor outlives any one terminal instance, so resuming into an empty
   * one would leave everything before the cursor blank. Returns whether the
   * replay resumed; when false the caller must clear its terminal.
   */
  async attach(botId: string, resume: boolean): Promise<AttachResult> {
    const after = resume ? this.cursors.get(botId) : undefined;
    const body: RequestBody =
      after === undefined
        ? { type: "attach", bot_id: botId }
        : { type: "attach", bot_id: botId, after_seq: after };
    const reply = await this.request(body, "attached");
    const resumed = after !== undefined && reply.resumed;
    if (!resumed) {
      this.cursors.set(botId, reply.seq);
    }
    return { seq: reply.seq, resumed };
  }

  // -------------------------------------------------------------------------

  private newReqId(): string {
    const id = this.nextReqId;
    this.nextReqId += 1;
    return String(id);
  }

  private setStatus(status: ConnectionStatus): void {
    if (this.status === status) {
      return;
    }
    this.status = status;
    for (const listener of this.statusListeners) {
      listener(status);
    }
  }

  private connect(): void {
    if (!this.running) {
      return;
    }
    // Keep an actionable handshake verdict visible while retrying in the
    // background. An explicit endpoint change clears it in `setEndpoint`.
    if (!isHandshakeVerdict(this.status)) {
      this.setStatus("connecting");
    }
    let ws: WebSocket;
    try {
      ws = new WebSocket(`ws://${this.endpoint.host}:${this.endpoint.port}/ws`);
    } catch {
      this.scheduleReconnect();
      return;
    }
    this.ws = ws;
    ws.addEventListener("open", () => {
      void this.handshake(ws);
    });
    ws.addEventListener("message", (event: MessageEvent) => {
      if (typeof event.data === "string") {
        this.handleFrame(event.data);
      }
    });
    ws.addEventListener("close", () => {
      if (this.ws !== ws) {
        return;
      }
      this.ws = null;
      this.failPending(new DaemonError("disconnected", "connection closed"));
      // A handshake verdict outlives the socket it was delivered on: reporting
      // the reconnect as a plain `disconnected` would hide the reason the
      // reconnect keeps failing.
      if (!isHandshakeVerdict(this.status)) {
        this.setStatus("disconnected");
      }
      this.scheduleReconnect();
    });
  }

  private async handshake(ws: WebSocket): Promise<void> {
    let token = "";
    try {
      token = await this.getToken();
    } catch {
      token = "";
    }
    if (this.ws !== ws || ws.readyState !== WebSocket.OPEN) {
      return;
    }
    try {
      const reply = await this.sendRawOn(ws, {
        type: "hello",
        protocol_version: PROTOCOL_VERSION,
        token,
        client: CLIENT_ID,
      });
      if (replyIs(reply, "error")) {
        throw new DaemonError(reply.code, reply.message);
      }
      if (!replyIs(reply, "hello_ok")) {
        throw new DaemonError("protocol_error", `expected hello_ok, got ${reply.type}`);
      }
      this.capabilities = reply.capabilities;
      this.serverVersion = reply.server_version;
      this.grants = reply.grants;
      this.deviceId = reply.device_id;
      this.backoffMs = MIN_BACKOFF_MS;
      this.setStatus("connected");
    } catch (error) {
      if (error instanceof DaemonError) {
        const verdict = HANDSHAKE_VERDICTS[error.code];
        if (verdict !== undefined) {
          this.setStatus(verdict);
        }
      }
      if (this.ws === ws) {
        ws.close();
      }
    }
  }

  private sendRequest(body: RequestBody): Promise<ServerReply> {
    if (this.status !== "connected") {
      return Promise.reject(new DaemonError("disconnected", "not connected to daemon"));
    }
    const ws = this.ws;
    if (ws === null || ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new DaemonError("disconnected", "not connected to daemon"));
    }
    return this.sendRawOn(ws, body);
  }

  private sendRawOn(ws: WebSocket, body: ClientRequestBody): Promise<ServerReply> {
    return new Promise<ServerReply>((resolve, reject) => {
      const reqId = this.newReqId();
      this.pending.set(reqId, { resolve, reject });
      try {
        ws.send(JSON.stringify({ ...body, req_id: reqId }));
      } catch (error) {
        this.pending.delete(reqId);
        reject(
          error instanceof Error ? error : new DaemonError("disconnected", "failed to send frame"),
        );
      }
    });
  }

  private handleFrame(raw: string): void {
    const message = parseServerMessage(raw);
    if (message === null) {
      return;
    }
    if (isReply(message)) {
      const pending = this.pending.get(message.req_id);
      if (pending !== undefined) {
        this.pending.delete(message.req_id);
        pending.resolve(message);
      }
      return;
    }
    this.dispatchPush(message);
  }

  private dispatchPush(push: ServerPush): void {
    // The replay cursor is the client's own state, so it is tracked here
    // rather than in the shared router.
    if (push.type === "term") {
      this.cursors.set(push.bot_id, push.seq);
    }
    dispatchPush(this.handlers, push);
  }

  private failPending(error: Error): void {
    const entries = [...this.pending.values()];
    this.pending.clear();
    for (const entry of entries) {
      entry.reject(error);
    }
  }

  private scheduleReconnect(): void {
    if (!this.running || this.reconnectTimer !== null) {
      return;
    }
    const delay = this.backoffMs;
    this.backoffMs = Math.min(this.backoffMs * 2, MAX_BACKOFF_MS);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, delay);
  }
}
