import { render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

interface MockClient {
  readonly capture: (event: string, properties: unknown) => void;
  readonly register: (properties: unknown) => void;
}

interface CaptureResultLike {
  readonly properties: Record<string, unknown>;
}

interface InitOptions {
  readonly loaded?: (client: MockClient) => void;
  readonly before_send?: (event: CaptureResultLike | null) => CaptureResultLike | null;
}

const client = vi.hoisted(() => ({
  capture: vi.fn<(event: string, properties: unknown) => void>(),
  captureException: vi.fn<(error: unknown, properties: unknown) => void>(),
  init: vi.fn<(token: string, options: InitOptions) => void>(),
  register: vi.fn<(properties: unknown) => void>(),
}));

vi.mock("posthog-js", () => ({ default: client }));

function Broken(): ReactElement {
  throw new Error("render failed");
}

afterEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  vi.unstubAllEnvs();
});

describe("analytics", () => {
  it.each([
    { token: "", host: "" },
    { token: "phc_test", host: "" },
    { token: "", host: "https://example.com" },
    { token: "  ", host: "https://example.com" },
    { token: "phc_test", host: "  " },
  ])("does nothing with incomplete PostHog configuration: %j", async ({ token, host }) => {
    vi.stubEnv("VITE_POSTHOG_PROJECT_TOKEN", token);
    vi.stubEnv("VITE_POSTHOG_HOST", host);
    const { AnalyticsProvider, capture, captureException } = await import("./analytics");

    render(
      <AnalyticsProvider>
        <span>App content</span>
      </AnalyticsProvider>,
    );
    capture("project_created", {});
    captureException(new Error("ignored"), "project_create");

    expect(screen.getByText("App content")).toBeInTheDocument();
    expect(client.init).not.toHaveBeenCalled();
    expect(client.capture).not.toHaveBeenCalled();
    expect(client.captureException).not.toHaveBeenCalled();
  });

  it("initializes private-by-default tracking and captures typed events", async () => {
    vi.stubEnv("VITE_POSTHOG_PROJECT_TOKEN", "phc_test");
    vi.stubEnv("VITE_POSTHOG_HOST", "https://eu.i.posthog.com");
    vi.stubEnv("VITE_APP_VERSION", "1.2.3");
    client.init.mockImplementation((_token, options) => {
      options.loaded?.(client);
    });
    const { AnalyticsProvider, capture, captureException } = await import("./analytics");

    render(
      <AnalyticsProvider>
        <span>Tracked content</span>
      </AnalyticsProvider>,
    );
    capture("search_opened", { source: "sidebar" });
    const error = new Error("request failed");
    captureException(error, "daemon_state_load");

    expect(screen.getByText("Tracked content")).toBeInTheDocument();
    expect(client.init).toHaveBeenCalledWith(
      "phc_test",
      expect.objectContaining({
        api_host: "https://eu.i.posthog.com",
        autocapture: false,
        capture_exceptions: true,
        capture_pageleave: false,
        capture_pageview: false,
        disable_session_recording: true,
        advanced_disable_feature_flags: true,
        person_profiles: "identified_only",
      }),
    );
    expect(client.capture).toHaveBeenCalledWith("app_launched", {
      app_version: "1.2.3",
      runtime: "browser",
    });
    // Registered too, so autocaptured exceptions carry the version as well.
    expect(client.register).toHaveBeenCalledWith({
      app_version: "1.2.3",
      runtime: "browser",
    });
    expect(client.capture).toHaveBeenCalledWith("search_opened", { source: "sidebar" });
    // The error goes through untouched: PostHog builds the stacktrace from it
    // and `before_send` does the scrubbing.
    expect(client.captureException).toHaveBeenCalledWith(error, {
      operation: "daemon_state_load",
      error_kind: "Error",
    });
  });

  it("keeps the real failure detail on a captured exception", async () => {
    vi.stubEnv("VITE_POSTHOG_PROJECT_TOKEN", "phc_test");
    vi.stubEnv("VITE_POSTHOG_HOST", "https://eu.i.posthog.com");
    const { captureException } = await import("./analytics");
    const { DaemonError } = await import("./protocol/connection");

    captureException(new DaemonError("not_found", "no such project"), "project_create", {
      phase: "request",
    });

    const [error, properties] = client.captureException.mock.calls.at(-1) ?? [];
    expect(error).toBeInstanceOf(Error);
    if (!(error instanceof Error)) {
      throw new Error("expected the original error");
    }
    expect(error.message).toBe("no such project");
    expect(properties).toEqual({
      operation: "project_create",
      error_kind: "DaemonError",
      daemon_code: "not_found",
      phase: "request",
    });
  });

  it("redacts home paths and tokens from every outgoing exception", async () => {
    vi.stubEnv("VITE_POSTHOG_PROJECT_TOKEN", "phc_test");
    vi.stubEnv("VITE_POSTHOG_HOST", "https://eu.i.posthog.com");
    await import("./analytics");

    const beforeSend = client.init.mock.calls.at(-1)?.[1].before_send;
    expect(beforeSend).toBeDefined();
    if (beforeSend === undefined) {
      throw new Error("expected a before_send hook");
    }
    const token = "a".repeat(64);
    const sent = beforeSend({
      properties: {
        $exception_values: [`ENOENT: /Users/alice/.gravity/bin/gravityd`],
        $exception_list: [{ type: "Error", value: `auth failed for ${token}` }],
        log_tail: `/Users/alice/.gravity/logs failed with ${token}`,
      },
    });

    expect(sent?.properties["$exception_values"]).toEqual([
      "ENOENT: /Users/~/.gravity/bin/gravityd",
    ]);
    expect(sent?.properties["$exception_list"]).toEqual([
      { type: "Error", value: "auth failed for [redacted]" },
    ]);
    expect(sent?.properties["log_tail"]).toBe("/Users/~/.gravity/logs failed with [redacted]");
  });

  it("shows a Gravity fallback for render failures", async () => {
    vi.stubEnv("VITE_POSTHOG_PROJECT_TOKEN", "phc_test");
    vi.stubEnv("VITE_POSTHOG_HOST", "https://eu.i.posthog.com");
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { AnalyticsProvider } = await import("./analytics");

    render(
      <AnalyticsProvider>
        <Broken />
      </AnalyticsProvider>,
    );

    expect(screen.getByText("Gravity stopped unexpectedly")).toBeInTheDocument();
    consoleError.mockRestore();
  });
});
