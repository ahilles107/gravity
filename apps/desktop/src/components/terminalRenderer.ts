import { WebglAddon } from "@xterm/addon-webgl";
import type { Terminal } from "@xterm/xterm";

/**
 * Delays before each attempt to get WebGL back after the browser dropped its
 * context. Contexts are shed when the GPU is under pressure, which is exactly
 * when the DOM renderer hurts most; a bounded retry keeps a transient loss
 * from costing the rest of the session, and a bounded one cannot thrash.
 */
const WEBGL_RETRY_DELAYS_MS: readonly number[] = [1_000, 2_000, 4_000];

/** A context that survived this long earned a fresh set of retries. */
const WEBGL_STABLE_MS = 60_000;

/** The slice of xterm the renderer needs, so tests can hand in a double. */
type RendererHost = Pick<Terminal, "loadAddon">;

/**
 * Without an accelerated renderer xterm falls back to its DOM renderer, which
 * rebuilds every visible row's markup per frame — far too slow for a
 * full-screen TUI that repaints constantly, and the main source of CPU burn
 * and scroll lag. WebGL does the same work on the GPU.
 *
 * The element records which renderer is active (`data-renderer`) and why an
 * accelerated one is not (`data-renderer-fallback`), for diagnostics.
 */
export function loadWebgl(term: RendererHost, element: HTMLElement, attempt = 0): void {
  try {
    const webgl = new WebglAddon();
    const loadedAt = Date.now();
    webgl.onContextLoss(() => {
      element.dataset.renderer = "dom";
      element.dataset.rendererFallback = "WebGL context lost";
      try {
        // Disposing the addon drops xterm back to the DOM renderer.
        webgl.dispose();
      } catch {
        // A dead context can make even disposal throw; nothing left to free.
      }
      const next = Date.now() - loadedAt >= WEBGL_STABLE_MS ? 0 : attempt;
      const delay = WEBGL_RETRY_DELAYS_MS[next];
      if (delay !== undefined) {
        setTimeout(() => {
          loadWebgl(term, element, next + 1);
        }, delay);
      }
    });
    term.loadAddon(webgl);
    element.dataset.renderer = "webgl";
    delete element.dataset.rendererFallback;
  } catch (error) {
    // No WebGL2 context available: the DOM renderer still works, just slower.
    element.dataset.renderer = "dom";
    element.dataset.rendererFallback =
      error instanceof Error ? error.message : "WebGL2 unavailable";
  }
}
