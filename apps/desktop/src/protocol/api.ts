import type { ConnectionStatus, Endpoint } from "./connection";
import type { Grant } from "./entities";
import type { PushOf, ReplyOf, ServerPushType, ServerReplyType } from "./messages";
import type { FireBody, RequestBody } from "./requests";

/** Terminal attach result: the server's sequence number and whether replay resumed. */
export interface AttachResult {
  readonly seq: number;
  readonly resumed: boolean;
}

/**
 * The control-plane surface the UI depends on. `DaemonClient` is the real
 * implementation; tests supply their own, so no component imports a socket.
 */
export interface DaemonApi {
  /** Changes whenever the configured daemon endpoint changes. */
  readonly connectionGeneration: number;
  readonly status: ConnectionStatus;
  readonly capabilities: readonly string[];
  readonly serverVersion: string;
  readonly grants: readonly Grant[];
  readonly deviceId: string | null;
  start(): void;
  setEndpoint(endpoint: Endpoint): void;
  getEndpoint(): Endpoint;
  hasGrant(grant: Grant): boolean;
  onStatus(listener: (status: ConnectionStatus) => void): () => void;
  on<K extends ServerPushType>(type: K, handler: (push: PushOf<K>) => void): () => void;
  request<K extends ServerReplyType>(body: RequestBody, expect: K): Promise<ReplyOf<K>>;
  fire(body: FireBody): void;
  /**
   * Attaches to a bot terminal. `resume` must be false whenever the caller
   * holds a fresh, empty terminal: only a caller that still has the previously
   * replayed screen may continue from its cursor.
   */
  attach(botId: string, resume: boolean): Promise<AttachResult>;
}
