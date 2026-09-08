import { useCallback, useState } from "react";
import type { ReactElement } from "react";
import { captureException } from "../../analytics";
import { useLoadOnConnect } from "../../hooks/useLoadOnConnect";
import type { DaemonApi } from "../../protocol/api";
import type { DaemonConfig, NotifyLevel } from "../../protocol/entities";
import { restartLocalDaemon } from "../../setup";
import { errText } from "../../util";
import ConfirmDialog from "../overlay/ConfirmDialog";

const AUTO_COMPACT_WINDOW_MIN = 100_000;
const AUTO_COMPACT_WINDOW_MAX = 1_000_000;

interface DaemonSettingsProps {
  readonly client: DaemonApi;
  readonly connected: boolean;
  readonly canControl: boolean;
  /** True only for the app-managed launchd daemon on this machine, the one we can bounce. */
  readonly canRestart: boolean;
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

type WindowParse = { readonly ok: true; readonly value: number | null } | { readonly ok: false };

/** Empty means "model default"; otherwise an integer within the accepted range. */
function parseWindow(draft: string): WindowParse {
  const trimmed = draft.trim();
  if (trimmed.length === 0) {
    return { ok: true, value: null };
  }
  const value = Number.parseInt(trimmed, 10);
  if (
    !Number.isInteger(value) ||
    value < AUTO_COMPACT_WINDOW_MIN ||
    value > AUTO_COMPACT_WINDOW_MAX
  ) {
    return { ok: false };
  }
  return { ok: true, value };
}

interface AutoCompactFormProps {
  readonly connected: boolean;
  readonly canControl: boolean;
  readonly dirty: boolean;
  readonly draft: string;
  readonly parsed: WindowParse;
  readonly saving: boolean;
  readonly onDraftChange: (draft: string) => void;
  readonly onSave: () => void;
}

function AutoCompactForm(props: AutoCompactFormProps): ReactElement {
  const { connected, canControl, dirty, draft, parsed, saving, onDraftChange, onSave } = props;
  return (
    <form
      className="settings-row settings-row-form"
      onSubmit={(event) => {
        event.preventDefault();
        onSave();
      }}
    >
      <div className="settings-row-text">
        <label className="settings-row-label" htmlFor="settings-compact-window">
          Auto-compact window
        </label>
        <div className="settings-row-help">
          Tokens a bot conversation may reach before Claude Code auto-compacts it. Accepted range is
          100,000–1,000,000; leave empty for the model default. Applies when a bot restarts.
        </div>
        <div className="settings-inline-fields">
          <input
            id="settings-compact-window"
            className="settings-number settings-tokens"
            inputMode="numeric"
            placeholder="model default"
            value={draft}
            disabled={!canControl}
            onChange={(event) => {
              onDraftChange(event.target.value);
            }}
          />
          {canControl ? (
            <button
              type="submit"
              className="btn btn-small btn-primary"
              disabled={!connected || saving || !dirty || !parsed.ok}
            >
              {saving ? "Saving…" : "Save"}
            </button>
          ) : null}
        </div>
        {parsed.ok ? null : (
          <div className="settings-row-error">Enter 100000–1000000, or leave empty.</div>
        )}
      </div>
    </form>
  );
}

interface RestartRowProps {
  readonly onToast: (level: NotifyLevel, title: string, body: string) => void;
}

/**
 * Bounces the launchd agent. Confirmed rather than immediate: bots run as
 * children of the daemon, so a restart cuts every session mid-turn.
 */
function RestartRow({ onToast }: RestartRowProps): ReactElement {
  const [confirming, setConfirming] = useState(false);
  const [restarting, setRestarting] = useState(false);

  const restart = async (): Promise<void> => {
    setConfirming(false);
    setRestarting(true);
    try {
      await restartLocalDaemon();
      onToast("info", "Daemon restarting", "The app reconnects once it is back up.");
    } catch (error) {
      captureException(error, "daemon_restart");
      onToast("error", "Restart failed", errText(error));
    } finally {
      setRestarting(false);
    }
  };

  return (
    <>
      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Restart daemon</div>
          <div className="settings-row-help">
            Bounces gravityd on this machine. Every bot session running right now is killed
            mid-turn.
          </div>
        </div>
        <button
          type="button"
          className="btn btn-small"
          disabled={restarting}
          onClick={() => {
            setConfirming(true);
          }}
        >
          {restarting ? "Restarting…" : "Restart"}
        </button>
      </div>
      {confirming ? (
        <ConfirmDialog
          title="Restart the daemon?"
          body="Every bot session running right now is killed mid-turn and loses whatever work it had not written to disk. The daemon comes back within a few seconds and the supervisor resumes the bots, but any in-flight turn is gone."
          confirmLabel="Restart daemon"
          onConfirm={() => {
            void restart();
          }}
          onCancel={() => {
            setConfirming(false);
          }}
        />
      ) : null}
    </>
  );
}

function DaemonLaunchConfig({ config }: { readonly config: DaemonConfig }): ReactElement {
  const negotiated = config.port !== config.configured_port;
  return (
    <>
      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Bind addresses</div>
          <div className="settings-row-help">Change in gravityd.toml and restart the daemon.</div>
        </div>
        <span className="settings-value">{config.bind.join(", ")}</span>
      </div>

      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Port</div>
          <div className="settings-row-help">Also serves the bot bus at /mcp.</div>
          {negotiated ? (
            <div className="settings-row-error">
              Port {config.configured_port} was in use at startup, so the daemon took {config.port}.
              The bot bus moved with it: a Mac whose Claude Code policy allowlists only the
              configured /mcp URL drops the bus without saying so. Free {config.configured_port} and
              the app-managed daemon restarts onto it within a minute, or set negotiate_port = false
              in gravityd.toml to make a collision fail loudly instead.
            </div>
          ) : null}
        </div>
        <span className="settings-value">
          {negotiated
            ? `${String(config.port)} (not ${String(config.configured_port)})`
            : config.port}
        </span>
      </div>

      <div className="settings-row">
        <div className="settings-row-text">
          <div className="settings-row-label">Runtime</div>
          <div className="settings-row-help">The agent runtime bots are spawned with.</div>
        </div>
        <span className="settings-value">{config.runtime}</span>
      </div>
    </>
  );
}

/**
 * Daemon configuration over the control plane, rendered as rows inside the
 * connection pane. Only the auto-compact window is writable; bind/port/runtime
 * describe how the daemon was launched and change only via `gravityd.toml`
 * plus a restart.
 */
export default function DaemonSettings(props: DaemonSettingsProps): ReactElement {
  const { client, connected, canControl, canRestart, onToast } = props;
  const [config, setConfig] = useState<DaemonConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);

  const load = useCallback(async (): Promise<void> => {
    try {
      const reply = await client.request({ type: "get_config" }, "config");
      setConfig(reply.config);
      setDraft(
        reply.config.auto_compact_window === null ? "" : String(reply.config.auto_compact_window),
      );
      setError(null);
    } catch (err) {
      setError(errText(err));
    }
  }, [client]);

  useLoadOnConnect(connected, load);

  const parsed = parseWindow(draft);
  const dirty = config !== null && (!parsed.ok || parsed.value !== config.auto_compact_window);

  const save = async (): Promise<void> => {
    if (!parsed.ok) {
      return;
    }
    setSaving(true);
    try {
      const reply = await client.request(
        { type: "set_config", auto_compact_window: parsed.value },
        "config",
      );
      setConfig(reply.config);
      setDraft(
        reply.config.auto_compact_window === null ? "" : String(reply.config.auto_compact_window),
      );
      onToast("info", "Daemon updated", "The auto-compact window applies when a bot restarts.");
    } catch (err) {
      onToast("error", "Update failed", errText(err));
    } finally {
      setSaving(false);
    }
  };

  // The restart row keeps a fixed slot after the config rows: moving it as the
  // config loads would remount it and drop an open confirmation. It also
  // outlives a config that never loaded — a daemon that is wedged or
  // unreachable is exactly when bouncing it is worth offering.
  return (
    <>
      {configRows({
        canControl,
        config,
        connected,
        dirty,
        draft,
        error,
        parsed,
        saving,
        setDraft,
        save,
      })}
      {canRestart ? <RestartRow onToast={onToast} /> : null}
    </>
  );
}

interface ConfigRowsProps {
  readonly canControl: boolean;
  readonly config: DaemonConfig | null;
  readonly connected: boolean;
  readonly dirty: boolean;
  readonly draft: string;
  readonly error: string | null;
  readonly parsed: WindowParse;
  readonly saving: boolean;
  readonly setDraft: (draft: string) => void;
  readonly save: () => Promise<void>;
}

function configRows(props: ConfigRowsProps): ReactElement {
  const { canControl, config, connected, dirty, draft, error, parsed, saving, setDraft, save } =
    props;
  if (error !== null) {
    return <div className="muted">Failed to load daemon config: {error}</div>;
  }
  if (config === null) {
    return <div className="muted">{connected ? "Loading…" : "Not connected."}</div>;
  }
  return (
    <>
      <AutoCompactForm
        connected={connected}
        canControl={canControl}
        dirty={dirty}
        draft={draft}
        parsed={parsed}
        saving={saving}
        onDraftChange={setDraft}
        onSave={() => {
          void save();
        }}
      />
      <DaemonLaunchConfig config={config} />
    </>
  );
}
