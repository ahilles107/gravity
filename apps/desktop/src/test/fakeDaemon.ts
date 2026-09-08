import type { AttachResult, DaemonApi } from "../protocol/api";
import type { ConnectionStatus, Endpoint } from "../protocol/connection";
import type { Grant } from "../protocol/entities";
import type {
  PushOf,
  ReplyOf,
  ServerPushType,
  ServerReply,
  ServerReplyType,
} from "../protocol/messages";
import type { FireBody, RequestBody } from "../protocol/requests";
import { replyIs } from "../protocol/wire";

export interface RecordedRequest {
  readonly body: RequestBody;
  readonly expect: ServerReplyType;
}

/** Answers one request type. Throw to simulate a daemon error. */
type Responder = (body: RequestBody) => ServerReply;

type HandlerSets = {
  readonly [K in ServerPushType]: Set<(push: PushOf<K>) => void>;
};

function emptyHandlers(): HandlerSets {
  return {
    term: new Set(),
    bot_state: new Set(),
    message_new: new Set(),
    bot_updated: new Set(),
    project_updated: new Set(),
    activity_update: new Set(),
    delivery_update: new Set(),
    routine_run_update: new Set(),
    approval_pending: new Set(),
    notify: new Set(),
  };
}

/**
 * In-memory `DaemonApi` for component tests: requests are answered from a
 * per-type responder table and pushes can be emitted on demand. Replies are
 * narrowed with the production type guard, so no test needs a cast.
 */
export class FakeDaemon implements DaemonApi {
  connectionGeneration = 0;
  status: ConnectionStatus = "connected";
  capabilities: readonly string[] = ["terminal", "routines"];
  serverVersion = "0.1.0-test";
  grants: readonly Grant[] = ["read", "control"];
  deviceId: string | null = null;

  readonly requests: RecordedRequest[] = [];
  readonly fired: FireBody[] = [];
  attachResult: AttachResult = { seq: 0, resumed: false };
  /** The `resume` flag of every attach, in order. */
  readonly attachResumes: boolean[] = [];
  /** Holds attach replies until `releaseAttach`, to test teardown races. */
  deferAttach = false;
  started = false;

  private endpoint: Endpoint = { host: "127.0.0.1", port: 7777 };
  private heldAttaches: (() => void)[] = [];
  private readonly responders = new Map<string, Responder>();
  private readonly handlers: HandlerSets = emptyHandlers();
  private readonly statusListeners = new Set<(status: ConnectionStatus) => void>();

  /** Registers the reply for one request type; later calls replace earlier ones. */
  onRequest(type: RequestBody["type"], responder: Responder): this {
    this.responders.set(type, responder);
    return this;
  }

  emit<K extends ServerPushType>(type: K, push: PushOf<K>): void {
    for (const handler of this.handlers[type]) {
      handler(push);
    }
  }

  setStatus(status: ConnectionStatus): void {
    this.status = status;
    for (const listener of this.statusListeners) {
      listener(status);
    }
  }

  start(): void {
    this.started = true;
  }

  setEndpoint(endpoint: Endpoint): void {
    this.endpoint = endpoint;
    this.connectionGeneration += 1;
  }

  getEndpoint(): Endpoint {
    return this.endpoint;
  }

  hasGrant(grant: Grant): boolean {
    return this.grants.includes(grant);
  }

  onStatus(listener: (status: ConnectionStatus) => void): () => void {
    this.statusListeners.add(listener);
    return () => {
      this.statusListeners.delete(listener);
    };
  }

  on<K extends ServerPushType>(type: K, handler: (push: PushOf<K>) => void): () => void {
    const set = this.handlers[type];
    set.add(handler);
    return () => {
      set.delete(handler);
    };
  }

  request<K extends ServerReplyType>(body: RequestBody, expect: K): Promise<ReplyOf<K>> {
    this.requests.push({ body, expect });
    const responder = this.responders.get(body.type);
    if (responder === undefined) {
      return Promise.reject(new Error(`no fake responder for '${body.type}'`));
    }
    let reply: ServerReply;
    try {
      reply = responder(body);
    } catch (error) {
      return Promise.reject(error instanceof Error ? error : new Error(String(error)));
    }
    if (replyIs(reply, expect)) {
      return Promise.resolve(reply);
    }
    return Promise.reject(new Error(`fake replied '${reply.type}', expected '${expect}'`));
  }

  fire(body: FireBody): void {
    this.fired.push(body);
  }

  attach(_botId: string, resume: boolean): Promise<AttachResult> {
    this.attachResumes.push(resume);
    if (!this.deferAttach) {
      return Promise.resolve(this.attachResult);
    }
    return new Promise<AttachResult>((resolve) => {
      this.heldAttaches.push(() => {
        resolve(this.attachResult);
      });
    });
  }

  /** Answers every attach held back by `deferAttach`. */
  releaseAttaches(): void {
    const held = this.heldAttaches;
    this.heldAttaches = [];
    for (const resolve of held) {
      resolve();
    }
  }
}
