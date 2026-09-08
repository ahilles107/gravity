import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";

/** The subset of Ladle's generated meta.json this suite reads. */
interface LadleMeta {
  readonly stories: Readonly<Record<string, unknown>>;
}

// Assembled at runtime rather than written as one literal: the build output is
// gitignored, so a static path here reads as a broken import to dead-code
// analysis.
const META_PATH = join(dirname(fileURLToPath(import.meta.url)), "..", "..", ".ladle-build");

function storyIds(): readonly string[] {
  let raw: string;
  try {
    raw = readFileSync(join(META_PATH, "meta.json"), "utf8");
  } catch {
    throw new Error("No .ladle-build/meta.json — run `ladle build -o .ladle-build` first.");
  }
  const meta = JSON.parse(raw) as LadleMeta;
  const ids = Object.keys(meta.stories);
  if (ids.length === 0) {
    throw new Error("Ladle build contains no stories.");
  }
  // Sorted so the suite reports in a stable order regardless of build output.
  return ids.toSorted();
}

for (const id of storyIds()) {
  test(id, async ({ page }) => {
    // `mode=preview` drops Ladle's own sidebar and addon chrome, so the
    // snapshot contains the component and nothing else.
    await page.goto(`/?story=${id}&mode=preview`);
    await page.waitForLoadState("networkidle");
    // Bundled bot icons and webfonts both settle after load; without this the
    // first run captures a frame with fallback glyphs.
    await page.evaluate(() => document.fonts.ready.then(() => undefined));
    await expect(page).toHaveScreenshot(`${id}.png`);
  });
}
