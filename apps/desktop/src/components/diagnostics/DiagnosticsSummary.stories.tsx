import type { Story } from "@ladle/react";
import { diagnostics } from "../../test/fixtures";
import DiagnosticsSummary from "./DiagnosticsSummary";

export const Healthy: Story = () => (
  <DiagnosticsSummary
    diagnostics={diagnostics({ uptime_seconds: 90_061 })}
    serverVersion="0.6.0"
    capabilities={["terminal", "routines", "devices"]}
  />
);

export const Degraded: Story = () => (
  <DiagnosticsSummary
    diagnostics={diagnostics({
      stale_build: true,
      db_healthy: false,
      runtime: { kind: "claude", available: false, version: null },
      delivery_backlog: 12,
    })}
    serverVersion=""
    capabilities={[]}
  />
);
