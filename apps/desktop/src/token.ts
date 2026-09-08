import { invoke } from "@tauri-apps/api/core";
import { loadDeviceToken } from "./settings";
import { isTauri } from "./tauri";

/**
 * Resolves the daemon token: a stored device token (remote setup) wins,
 * otherwise the local token file via the Tauri shell.
 * Falls back to an empty string when running in a plain browser (vite dev).
 */
export async function readClientToken(): Promise<string> {
  const device = loadDeviceToken();
  if (device.length > 0) {
    return device;
  }
  if (!isTauri()) {
    return "";
  }
  try {
    const token: unknown = await invoke("read_client_token");
    return typeof token === "string" ? token : "";
  } catch {
    return "";
  }
}
