import { useCallback, useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import { capture, captureException } from "../../analytics";
import type { Endpoint } from "../../protocol/connection";
import { DEFAULT_ENDPOINT } from "../../settings";
import { daemonLogTail, installLocalDaemon, localDaemonEndpoint, probeDaemon } from "../../setup";
import type { SetupConnection } from "../../setup";

interface SetupScreenProps {
  /** Points the client at the daemon the wizard settled on and reconnects. */
  readonly onConnect: (connection: SetupConnection) => void;
}

type Phase =
  | { readonly kind: "probing" }
  | { readonly kind: "connecting" }
  | { readonly kind: "installing"; readonly waited: number }
  | { readonly kind: "error"; readonly message: string }
  // Carries the error it replaced so Back can restore it.
  | { readonly kind: "manual"; readonly previousError: string };

const POLL_ATTEMPTS = 30;
const POLL_INTERVAL_MS = 1000;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Where the wait ended: the endpoint that answered, or every port tried. */
type WaitResult =
  | { readonly kind: "ready"; readonly endpoint: Endpoint }
  | { readonly kind: "timeout"; readonly ports: readonly number[] };

/**
 * Waits for a freshly installed daemon to answer its health endpoint, and
 * reports the endpoint it answered on.
 */
async function waitForLocalDaemon(onWait: (waited: number) => void): Promise<WaitResult> {
  const ports = new Set<number>();
  for (let attempt = 0; attempt < POLL_ATTEMPTS; attempt += 1) {
    // The managed daemon may publish a negotiated port after launchd returns
    // from bootstrap, so resolve the endpoint again on every attempt.
    // oxlint-disable-next-line no-await-in-loop
    const endpoint = await localDaemonEndpoint();
    ports.add(endpoint.port);
    // oxlint-disable-next-line no-await-in-loop
    if ((await probeDaemon(endpoint)) !== null) {
      return { kind: "ready", endpoint };
    }
    onWait(attempt + 1);
    // oxlint-disable-next-line no-await-in-loop
    await sleep(POLL_INTERVAL_MS);
  }
  return { kind: "timeout", ports: [...ports] };
}

/**
 * First-run wizard: connects to the daemon on this machine, installing the
 * bundled one if nothing answers. When that cannot be made to work it falls
 * back to a typed-in address so the app is never unreachable. The parent
 * unmounts it as soon as the client connects.
 */
export default function SetupScreen(props: SetupScreenProps): ReactElement {
  const { onConnect } = props;
  const [phase, setPhase] = useState<Phase>({ kind: "probing" });

  // Cleared on unmount and set again on mount, so StrictMode's extra
  // mount/unmount/mount cycle leaves an in-flight attempt free to finish.
  const alive = useRef(true);

  const findOrInstallDaemon = useCallback(async (): Promise<void> => {
    setPhase({ kind: "probing" });
    const existing = await localDaemonEndpoint();
    if ((await probeDaemon(existing)) !== null) {
      if (alive.current) {
        setPhase({ kind: "connecting" });
        onConnect({ method: "local", endpoint: existing });
      }
      return;
    }
    if (!alive.current) {
      return;
    }
    setPhase({ kind: "installing", waited: 0 });
    capture("setup_install_started", {});
    try {
      await installLocalDaemon();
    } catch (error) {
      captureException(error, "setup_install", { phase: "install" });
      const message = error instanceof Error ? error.message : "daemon install failed";
      if (alive.current) {
        setPhase({ kind: "error", message });
      }
      return;
    }
    const result = await waitForLocalDaemon((waited) => {
      if (alive.current) {
        setPhase({ kind: "installing", waited });
      }
    });
    if (!alive.current) {
      return;
    }
    if (result.kind === "ready") {
      setPhase({ kind: "connecting" });
      onConnect({ method: "local", endpoint: result.endpoint });
      return;
    }
    // The same log tail the user is about to read: an install that stages
    // cleanly but never answers is only ever explained by the daemon's own log.
    const ports = result.ports.map(String).join(", ");
    const tail = await daemonLogTail();
    captureException(new Error("Local daemon did not become healthy"), "setup_install", {
      phase: "health_timeout",
      ports: result.ports,
      attempts: POLL_ATTEMPTS,
      log_tail: tail,
    });
    if (!alive.current) {
      return;
    }
    setPhase({
      kind: "error",
      message:
        `The daemon was installed but never answered on ${
          result.ports.length > 1 ? `ports ${ports}` : `port ${ports}`
        }.` + (tail.length > 0 ? `\n\n${tail}` : ""),
    });
  }, [onConnect]);

  // Probe exactly once, on first mount; the ref keeps a re-created `onConnect`
  // from re-triggering it.
  const probed = useRef(false);
  useEffect(() => {
    alive.current = true;
    if (!probed.current) {
      probed.current = true;
      void findOrInstallDaemon();
    }
    return () => {
      alive.current = false;
    };
  }, [findOrInstallDaemon]);

  return (
    <div className="setup-screen">
      <div className="setup-panel">
        <h1 className="setup-title">Gravity</h1>
        {phase.kind === "probing" ? (
          <p className="setup-note">Looking for a daemon on this Mac…</p>
        ) : null}
        {phase.kind === "connecting" ? (
          <p className="setup-note">Connecting to the daemon…</p>
        ) : null}
        {phase.kind === "installing" ? (
          <>
            <p className="setup-note">Installing the daemon on this Mac…</p>
            <p className="setup-detail">
              It is installed as a launchd agent in ~/Library/LaunchAgents and keeps running in the
              background after you quit Gravity.
            </p>
            {phase.waited > 0 ? (
              <p className="setup-detail">
                Waiting for it to answer — {String(phase.waited)}s of {String(POLL_ATTEMPTS)}s.
              </p>
            ) : null}
          </>
        ) : null}
        {phase.kind === "error" ? (
          <>
            <p className="setup-error">{phase.message}</p>
            <div className="setup-actions">
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => {
                  void findOrInstallDaemon();
                }}
              >
                Retry
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  setPhase({ kind: "manual", previousError: phase.message });
                }}
              >
                Connect to a different daemon
              </button>
            </div>
          </>
        ) : null}
        {phase.kind === "manual" ? (
          <ManualConnectForm
            onBack={() => {
              setPhase({ kind: "error", message: phase.previousError });
            }}
            onSubmit={(connection) => {
              setPhase({ kind: "connecting" });
              onConnect(connection);
            }}
          />
        ) : null}
      </div>
    </div>
  );
}

interface ManualConnectFormProps {
  readonly onBack: () => void;
  readonly onSubmit: (connection: SetupConnection) => void;
}

function ManualConnectForm(props: ManualConnectFormProps): ReactElement {
  const { onBack, onSubmit } = props;
  const [host, setHost] = useState(DEFAULT_ENDPOINT.host);
  const [port, setPort] = useState(String(DEFAULT_ENDPOINT.port));
  const [token, setToken] = useState("");

  const parsedPort = Number.parseInt(port, 10);
  const valid = host.trim().length > 0 && Number.isInteger(parsedPort) && parsedPort > 0;

  return (
    <form
      className="setup-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (!valid) {
          return;
        }
        const trimmed = token.trim();
        onSubmit({
          method: "remote",
          endpoint: { host: host.trim(), port: parsedPort },
          ...(trimmed.length > 0 ? { token: trimmed } : {}),
        });
      }}
    >
      <p className="setup-detail">
        Point Gravity at a daemon running elsewhere — a Mac mini or server on your tailnet. A device
        token is only needed when the daemon is on another machine.
      </p>
      <input
        aria-label="Daemon host"
        placeholder="Host (e.g. mini.tailnet.ts.net)"
        value={host}
        onChange={(event) => {
          setHost(event.target.value);
        }}
      />
      <input
        aria-label="Daemon port"
        placeholder="Port"
        inputMode="numeric"
        value={port}
        onChange={(event) => {
          setPort(event.target.value);
        }}
      />
      <input
        aria-label="Device token"
        placeholder="Device token (optional)"
        value={token}
        onChange={(event) => {
          setToken(event.target.value);
        }}
      />
      <div className="setup-form-actions">
        <button type="submit" className="btn btn-primary" disabled={!valid}>
          Connect
        </button>
        <button type="button" className="btn" onClick={onBack}>
          Back
        </button>
      </div>
    </form>
  );
}
