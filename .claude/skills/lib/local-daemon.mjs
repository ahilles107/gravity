import { readFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

/**
 * Check a disposable daemon before reading credentials or opening a socket.
 * @param {string | undefined} home
 * @param {string} endpoint
 * @returns {{ home: string, endpoint: URL }}
 */
export function localDaemon(home, endpoint) {
  const url = new URL(endpoint);
  if (
    url.protocol !== "ws:" ||
    url.hostname !== "127.0.0.1" ||
    url.pathname !== "/ws" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    !url.port ||
    url.port === "49777"
  ) {
    throw new Error("Use ws://127.0.0.1:<test-port>/ws; production port 49777 is forbidden");
  }
  if (!home) {
    throw new Error("Set GRAVITY_HOME to the disposable daemon home created for this run");
  }
  const canonicalHome = realpathSync(home);
  const installedHome = resolve(realpathSync(homedir()), ".gravity");
  let canonicalInstalledHome = installedHome;
  try {
    canonicalInstalledHome = realpathSync(installedHome);
  } catch (error) {
    if (!(error instanceof Error) || !("code" in error) || error.code !== "ENOENT") {
      throw error;
    }
  }
  if (canonicalHome === canonicalInstalledHome) {
    throw new Error("The installed daemon home is forbidden; create a disposable home");
  }
  const port = readFileSync(join(canonicalHome, "gravityd.port"), "utf8").trim();
  if (port !== url.port) {
    throw new Error("Endpoint does not match the disposable daemon's published port");
  }
  return { home: canonicalHome, endpoint: url };
}
