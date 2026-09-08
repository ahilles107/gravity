import type { Endpoint } from "./protocol/connection";

const STORAGE_KEY = "gravity.connection";

export const DEFAULT_ENDPOINT: Endpoint = { host: "127.0.0.1", port: 49777 };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function envString(key: string): string | undefined {
  const value: unknown = import.meta.env[key];
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

/**
 * The workspace-private daemon `scripts/dev.sh` starts, injected at build time.
 * It seeds a fresh client so a dev window connects to its own daemon instead of
 * the installed one; anything stored in localStorage still wins.
 */
function devEndpoint(): Endpoint | undefined {
  const port = Number(envString("VITE_GRAVITY_DEV_PORT"));
  if (!Number.isInteger(port) || port <= 0) {
    return undefined;
  }
  return { host: envString("VITE_GRAVITY_DEV_HOST") ?? "127.0.0.1", port };
}

/** Loads the daemon endpoint from localStorage (a laptop can point at a Tailscale host). */
export function loadEndpoint(): Endpoint {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === null) {
      return devEndpoint() ?? DEFAULT_ENDPOINT;
    }
    const parsed: unknown = JSON.parse(raw);
    if (
      isRecord(parsed) &&
      typeof parsed["host"] === "string" &&
      parsed["host"].length > 0 &&
      typeof parsed["port"] === "number" &&
      Number.isInteger(parsed["port"])
    ) {
      return { host: parsed["host"], port: parsed["port"] };
    }
  } catch {
    // fall through to default
  }
  return devEndpoint() ?? DEFAULT_ENDPOINT;
}

export function saveEndpoint(endpoint: Endpoint): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(endpoint));
  } catch {
    // localStorage unavailable; setting is session-only
  }
}

const DEVICE_TOKEN_KEY = "gravity.device-token";

/**
 * A device-scoped token entered during remote setup. Preferred over the local
 * token file, which only exists on the machine running the daemon.
 */
export function loadDeviceToken(): string {
  const fallback = envString("VITE_GRAVITY_DEV_TOKEN") ?? "";
  try {
    return localStorage.getItem(DEVICE_TOKEN_KEY) ?? fallback;
  } catch {
    return fallback;
  }
}

export function saveDeviceToken(token: string): void {
  try {
    if (token.length === 0) {
      localStorage.removeItem(DEVICE_TOKEN_KEY);
    } else {
      localStorage.setItem(DEVICE_TOKEN_KEY, token);
    }
  } catch {
    // localStorage unavailable; token is session-only
  }
}

const SETUP_KEY = "gravity.setup-complete";

/** True once this install has connected to a daemon at least once. */
export function loadSetupComplete(): boolean {
  try {
    const raw = localStorage.getItem(SETUP_KEY);
    if (raw === null) {
      // A dev build points at its own daemon already, so skip the wizard.
      return devEndpoint() !== undefined;
    }
    return raw === "true";
  } catch {
    return devEndpoint() !== undefined;
  }
}

export function markSetupComplete(): void {
  try {
    localStorage.setItem(SETUP_KEY, "true");
  } catch {
    // localStorage unavailable; the wizard may show again next launch
  }
}

const PINNED_KEY = "gravity.pinned-bots";

/** Loads the pinned bot ids, most recently pinned last, in display order. */
export function loadPinnedBotIds(): readonly string[] {
  try {
    const raw = localStorage.getItem(PINNED_KEY);
    if (raw === null) {
      return [];
    }
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return parsed.filter((id): id is string => typeof id === "string" && id.length > 0);
    }
  } catch {
    // fall through to no pins
  }
  return [];
}

export function savePinnedBotIds(ids: readonly string[]): void {
  try {
    localStorage.setItem(PINNED_KEY, JSON.stringify(ids));
  } catch {
    // localStorage unavailable; pins are session-only
  }
}

const LAST_USED_BOT_KEY = "gravity.last-used-bot";

/** Loads the bot that was open when the app was last used. */
export function loadLastUsedBotId(): string | undefined {
  try {
    const botId = localStorage.getItem(LAST_USED_BOT_KEY);
    return botId === null || botId.length === 0 ? undefined : botId;
  } catch {
    return undefined;
  }
}

export function saveLastUsedBotId(botId: string): void {
  try {
    localStorage.setItem(LAST_USED_BOT_KEY, botId);
  } catch {
    // localStorage unavailable; selection is session-only
  }
}

const BOT_INFO_PANEL_KEY = "gravity.bot-info-panel";

export interface BotInfoPanelSettings {
  readonly collapsed: boolean;
  readonly width: number;
}

export const DEFAULT_BOT_INFO_PANEL: BotInfoPanelSettings = {
  collapsed: false,
  width: 360,
};

export const MIN_BOT_INFO_PANEL_WIDTH = 280;

/** Loads the bot inspector layout shared by every bot view. */
export function loadBotInfoPanel(): BotInfoPanelSettings {
  try {
    const raw = localStorage.getItem(BOT_INFO_PANEL_KEY);
    if (raw === null) {
      return DEFAULT_BOT_INFO_PANEL;
    }
    const parsed: unknown = JSON.parse(raw);
    if (
      isRecord(parsed) &&
      typeof parsed["collapsed"] === "boolean" &&
      typeof parsed["width"] === "number" &&
      Number.isFinite(parsed["width"]) &&
      parsed["width"] >= MIN_BOT_INFO_PANEL_WIDTH
    ) {
      return { collapsed: parsed["collapsed"], width: parsed["width"] };
    }
  } catch {
    // fall through to the default layout
  }
  return DEFAULT_BOT_INFO_PANEL;
}

export function saveBotInfoPanel(settings: BotInfoPanelSettings): void {
  try {
    localStorage.setItem(BOT_INFO_PANEL_KEY, JSON.stringify(settings));
  } catch {
    // localStorage unavailable; layout is session-only
  }
}

const UNREAD_KEY = "gravity.unread";

/**
 * One bot's badge state. `seenAt` is how far the user has caught up: the epoch
 * milliseconds of the newest item already read or already counted, so a
 * restarted client can tell what it has accounted for from what it has not.
 */
export interface UnreadEntry {
  readonly count: number;
  readonly seenAt: number;
}

export type UnreadState = Readonly<Record<string, UnreadEntry>>;

function entryFrom(value: unknown): UnreadEntry | undefined {
  if (
    isRecord(value) &&
    typeof value["count"] === "number" &&
    Number.isFinite(value["count"]) &&
    value["count"] >= 0 &&
    typeof value["seenAt"] === "number" &&
    Number.isFinite(value["seenAt"])
  ) {
    return { count: value["count"], seenAt: value["seenAt"] };
  }
  return undefined;
}

/**
 * Loads the unread badges. They live here rather than on the daemon because a
 * bot's replies never touch the message bus — the daemon only knows what a bot
 * has yet to consume, which says nothing about what the user has read.
 */
export function loadUnread(): UnreadState {
  try {
    const raw = localStorage.getItem(UNREAD_KEY);
    if (raw === null) {
      return {};
    }
    const parsed: unknown = JSON.parse(raw);
    if (!isRecord(parsed)) {
      return {};
    }
    const state: Record<string, UnreadEntry> = {};
    for (const [botId, value] of Object.entries(parsed)) {
      const entry = entryFrom(value);
      if (entry !== undefined) {
        state[botId] = entry;
      }
    }
    return state;
  } catch {
    // fall through to no badges
  }
  return {};
}

export function saveUnread(state: UnreadState): void {
  try {
    localStorage.setItem(UNREAD_KEY, JSON.stringify(state));
  } catch {
    // localStorage unavailable; badges are session-only
  }
}
