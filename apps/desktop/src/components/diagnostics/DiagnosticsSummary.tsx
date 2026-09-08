import type { ReactElement } from "react";
import type { Diagnostics } from "../../protocol/entities";

interface DiagnosticsSummaryProps {
  readonly diagnostics: Diagnostics;
  readonly serverVersion: string;
  readonly capabilities: readonly string[];
}

const SECONDS_PER_DAY = 86_400;
const SECONDS_PER_HOUR = 3600;
const SECONDS_PER_MINUTE = 60;

function fmtUptime(seconds: number): string {
  const days = Math.floor(seconds / SECONDS_PER_DAY);
  const hours = Math.floor((seconds % SECONDS_PER_DAY) / SECONDS_PER_HOUR);
  const minutes = Math.floor((seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  if (days > 0) {
    return `${days}d ${hours}h ${minutes}m`;
  }
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

function runtimeLabel(diagnostics: Diagnostics): string {
  const { kind, version, available } = diagnostics.runtime;
  const suffix = available ? " (available)" : " (unavailable)";
  return typeof version === "string" ? `${kind} ${version}${suffix}` : `${kind}${suffix}`;
}

export default function DiagnosticsSummary(props: DiagnosticsSummaryProps): ReactElement {
  const { diagnostics, serverVersion, capabilities } = props;
  return (
    <dl className="info-meta diag-grid">
      <dt>Daemon version</dt>
      <dd>{diagnostics.daemon_version}</dd>
      <dt>Protocol version</dt>
      <dd>{diagnostics.protocol_version}</dd>
      <dt>Build</dt>
      <dd className={diagnostics.stale_build ? "err-text" : "ok-text"}>
        {diagnostics.stale_build
          ? "stale — restart the daemon to pick up the new build"
          : "current"}
      </dd>
      <dt>Server (handshake)</dt>
      <dd>{serverVersion.length > 0 ? serverVersion : "–"}</dd>
      <dt>Capabilities</dt>
      <dd>{capabilities.length > 0 ? capabilities.join(", ") : "–"}</dd>
      <dt>Database</dt>
      <dd className={diagnostics.db_healthy ? "ok-text" : "err-text"}>
        {diagnostics.db_healthy ? "healthy" : "unhealthy"}
      </dd>
      <dt>Runtime</dt>
      <dd className={diagnostics.runtime.available ? "ok-text" : "err-text"}>
        {runtimeLabel(diagnostics)}
      </dd>
      <dt>Delivery backlog</dt>
      <dd>{diagnostics.delivery_backlog}</dd>
      <dt>Active bots</dt>
      <dd>{diagnostics.active_bots}</dd>
      <dt>Uptime</dt>
      <dd>{fmtUptime(diagnostics.uptime_seconds)}</dd>
    </dl>
  );
}
