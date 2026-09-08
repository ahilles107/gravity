const MACOS_PLATFORM = "darwin-aarch64";
const APP_ARCHIVE_SUFFIX = ".app.tar.gz";
const DMG_SUFFIX = ".dmg";

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function resolveMacDownloadUrl(manifest: unknown): string | null {
  if (!isRecord(manifest)) {
    return null;
  }

  const platforms = manifest["platforms"];
  if (!isRecord(platforms)) {
    return null;
  }

  const macos = platforms[MACOS_PLATFORM];
  if (!isRecord(macos)) {
    return null;
  }

  const archiveUrl = macos["url"];
  if (typeof archiveUrl !== "string" || !archiveUrl.endsWith(APP_ARCHIVE_SUFFIX)) {
    return null;
  }

  const downloadUrl = new URL(archiveUrl);
  if (downloadUrl.protocol !== "https:") {
    return null;
  }

  downloadUrl.pathname = `${downloadUrl.pathname.slice(0, -APP_ARCHIVE_SUFFIX.length)}${DMG_SUFFIX}`;
  return downloadUrl.toString();
}
