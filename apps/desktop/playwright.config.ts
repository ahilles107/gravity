import { defineConfig } from "@playwright/test";

const PORT = 61_001;
const BASE_URL = `http://127.0.0.1:${PORT}`;

// Baselines are byte-compared, so they are only ever valid for the exact
// browser, fonts and renderer that produced them — the pinned Playwright
// container in .github/workflows/visual.yml. A macOS or bare-Linux checkout
// renders different glyphs, so writing them anywhere else would commit noise
// that CI immediately rejects. `scripts/vr-accept.sh` pulls the CI-produced
// set instead.
const UPDATING = process.argv.some((arg) => arg.startsWith("--update-snapshots"));
if (UPDATING && process.env.CI !== "true") {
  throw new Error(
    "Refusing to write visual baselines outside CI: they are environment-specific.\n" +
      "Push the change, let the `visual` workflow fail, then run `scripts/vr-accept.sh`.",
  );
}

export default defineConfig({
  testDir: "tests/visual",
  // One flat directory of PNGs named after the Ladle story id. No platform
  // suffix: exactly one environment is allowed to produce these.
  snapshotPathTemplate: "{testDir}/__screenshots__/{arg}{ext}",
  // A missing baseline is a failure, never a silent write.
  updateSnapshots: "none",
  fullyParallel: true,
  forbidOnly: true,
  retries: 0,
  reporter: [["line"], ["html", { open: "never" }]],
  use: {
    baseURL: BASE_URL,
    viewport: { width: 1024, height: 720 },
    deviceScaleFactor: 1,
    colorScheme: "dark",
    // Fixture timestamps render through `toLocaleTimeString`, so both of these
    // have to be pinned or every story with a date drifts by machine.
    timezoneId: "UTC",
    locale: "en-US",
  },
  expect: {
    toHaveScreenshot: { maxDiffPixels: 0, animations: "disabled", caret: "hide", scale: "css" },
  },
  webServer: {
    // Bare binary name: the suite is always started through a package-manager
    // `run` script, so node_modules/.bin is already on PATH and the container
    // does not need pnpm installed.
    // --host is explicit because `ladle preview` otherwise binds localhost as
    // IPv6 only, which the IPv4 baseURL cannot reach.
    command: `ladle preview -o .ladle-build --host 127.0.0.1 --port ${PORT}`,
    url: BASE_URL,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
