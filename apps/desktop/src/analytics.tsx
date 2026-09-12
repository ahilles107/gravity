import { PostHogErrorBoundary, PostHogProvider } from "@posthog/react";
import type { ReactElement, ReactNode } from "react";
import posthogClient from "posthog-js";
import type { CaptureResult } from "posthog-js";
import { DaemonError } from "./protocol/connection";
import { LOG_LIMIT, MESSAGE_LIMIT, redact, redactTail } from "./redact";
import { isTauri } from "./tauri";

interface AnalyticsEvents {
  readonly app_launched: {
    readonly app_version: string;
    readonly runtime: "browser" | "tauri";
  };
  readonly bot_created: Record<string, never>;
  readonly control_center_opened: Record<string, never>;
  readonly decision_answered: {
    readonly picked_option: boolean;
  };
  readonly decision_deleted: Record<string, never>;
  readonly decision_held: Record<string, never>;
  readonly decision_published: {
    readonly count: number;
    readonly notified: number;
  };
  readonly bot_deleted: Record<string, never>;
  readonly daemon_connected: {
    readonly can_control: boolean;
    readonly connection_type: "local" | "remote";
  };
  readonly device_created: {
    readonly capabilities: readonly string[];
  };
  readonly device_revoked: Record<string, never>;
  readonly project_created: Record<string, never>;
  readonly project_lead_set: {
    readonly cleared: boolean;
  };
  readonly project_deleted: Record<string, never>;
  readonly project_renamed: Record<string, never>;
  readonly routine_created: {
    readonly trigger_type: string;
  };
  readonly routine_run: Record<string, never>;
  readonly routine_toggled: {
    readonly enabled: boolean;
  };
  readonly search_opened: {
    readonly source: "command_palette" | "sidebar";
  };
  readonly settings_opened: {
    readonly category: string;
  };
  readonly setup_install_started: Record<string, never>;
  readonly setup_method_selected: {
    readonly method: "local" | "remote";
  };
}

export type ErrorOperation =
  | "app_update"
  | "bot_create"
  | "bot_delete"
  | "daemon_restart"
  | "decision_answer"
  | "decision_comment"
  | "decision_delete"
  | "decision_hold"
  | "decision_list"
  | "decision_publish"
  | "decision_update"
  | "daemon_state_load"
  | "daemon_update"
  | "device_create"
  | "device_list"
  | "device_revoke"
  | "project_create"
  | "project_delete"
  | "project_lead_set"
  | "project_rename"
  | "routine_create"
  | "routine_list"
  | "routine_run"
  | "routine_toggle"
  | "setup_install"
  | "tag_list"
  | "tag_update"
  | "terminal_file_drop"
  | "window_shortcut";

const projectToken = import.meta.env.VITE_POSTHOG_PROJECT_TOKEN;
const apiHost = import.meta.env.VITE_POSTHOG_HOST;
const analyticsEnabled =
  typeof projectToken === "string" &&
  projectToken.trim().length > 0 &&
  typeof apiHost === "string" &&
  apiHost.trim().length > 0;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/** Exception bodies are free text, so they are scrubbed rather than trusted. */
function redactExceptionEntry(entry: unknown): unknown {
  if (!isRecord(entry) || typeof entry["value"] !== "string") {
    return entry;
  }
  return { ...entry, value: redact(entry["value"], MESSAGE_LIMIT) };
}

/**
 * Scrubs every exception before it leaves the app.
 *
 * This is the single choke point on purpose: PostHog autocaptures unhandled
 * errors and rejections itself, so redacting only inside `captureException`
 * would leave the majority of exceptions untouched. Stack frame filenames are
 * left alone — in a bundled build they are `tauri://localhost` URLs carrying
 * nothing personal, and rewriting them would break symbolication.
 */
function redactEvent(event: CaptureResult | null): CaptureResult | null {
  if (event === null) {
    return null;
  }
  const properties: Record<string, unknown> = event.properties;
  const values = properties["$exception_values"];
  if (Array.isArray(values)) {
    properties["$exception_values"] = values.map((value: unknown) =>
      typeof value === "string" ? redact(value, MESSAGE_LIMIT) : value,
    );
  }
  const list = properties["$exception_list"];
  if (Array.isArray(list)) {
    properties["$exception_list"] = list.map(redactExceptionEntry);
  }
  const logTail = properties["log_tail"];
  if (typeof logTail === "string") {
    properties["log_tail"] = redactTail(logTail, LOG_LIMIT);
  }
  return event;
}

if (analyticsEnabled) {
  posthogClient.init(projectToken, {
    api_host: apiHost,
    defaults: "2026-05-30",
    autocapture: false,
    capture_exceptions: true,
    capture_pageleave: false,
    capture_pageview: false,
    disable_session_recording: true,
    advanced_disable_feature_flags: true,
    person_profiles: "identified_only",
    before_send: redactEvent,
    loaded: (client) => {
      const launch: AnalyticsEvents["app_launched"] = {
        app_version: import.meta.env.VITE_APP_VERSION ?? "development",
        runtime: isTauri() ? "tauri" : "browser",
      };
      // Registered as well as captured: an autocaptured exception is not an
      // `app_launched`, and without this it arrives with no version at all.
      client.register(launch);
      client.capture("app_launched", launch);
    },
  });
}

/** Daemon facts worth having on every later event, exceptions included. */
export interface DaemonContext {
  readonly can_control: boolean;
  readonly connection_type: "local" | "remote";
  readonly daemon_version: string;
}

export function registerDaemonContext(context: DaemonContext): void {
  if (analyticsEnabled) {
    posthogClient.register(context);
  }
}

export function capture<Event extends keyof AnalyticsEvents>(
  event: Event,
  properties: AnalyticsEvents[Event],
): void {
  if (analyticsEnabled) {
    posthogClient.capture(event, properties);
  }
}

/** Extra facts about a failure, beyond the error itself. */
export interface ErrorContext {
  /** How far a multi-step operation got before it failed. */
  readonly phase?: string;
  /** Daemon log tail, for failures the app cannot explain on its own. */
  readonly log_tail?: string;
  readonly port?: number;
  /** Every port an attempt was made against, when the daemon's port can move. */
  readonly ports?: readonly number[];
  readonly attempts?: number;
}

function errorKind(error: unknown): string {
  return error instanceof Error ? error.name : typeof error;
}

/**
 * Reports a failure with its real message and stack.
 *
 * The error is passed through untouched so PostHog can build a proper
 * stacktrace from it; `before_send` scrubs the result. Replacing the message
 * here instead — as this used to — produced issues reading only
 * "setup_install failed", which grouped every cause into one useless bucket.
 */
export function captureException(
  error: unknown,
  operation: ErrorOperation,
  context: ErrorContext = {},
): void {
  if (!analyticsEnabled) {
    return;
  }
  const properties: Record<string, unknown> = {
    ...context,
    operation,
    error_kind: errorKind(error),
  };
  if (error instanceof DaemonError) {
    properties["daemon_code"] = error.code;
  }
  posthogClient.captureException(error, properties);
}

function ErrorFallback(): ReactElement {
  return (
    <div className="setup-screen">
      <div className="setup-panel">
        <h1 className="setup-title">Gravity stopped unexpectedly</h1>
        <p className="setup-note">Restart the app to continue.</p>
      </div>
    </div>
  );
}

interface AnalyticsProviderProps {
  readonly children: ReactNode;
}

export function AnalyticsProvider({ children }: AnalyticsProviderProps): ReactElement {
  if (!analyticsEnabled) {
    return <>{children}</>;
  }
  return (
    <PostHogProvider client={posthogClient}>
      <PostHogErrorBoundary fallback={ErrorFallback}>{children}</PostHogErrorBoundary>
    </PostHogProvider>
  );
}
