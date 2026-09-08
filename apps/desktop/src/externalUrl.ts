import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

function isWebUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

/**
 * Opens an HTTP(S) URL in the user's default browser.
 *
 * Rejects rather than swallowing failures. Opening a link is something the
 * user asked for by clicking, so a link that goes nowhere has to be
 * reportable; the caller decides how loudly to say so.
 */
export async function openExternalUrl(url: string): Promise<void> {
  if (!isWebUrl(url)) {
    throw new Error(`refusing to open a non-web URL: ${url}`);
  }
  if (isTauri()) {
    await invoke("open_external_url", { url });
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}
