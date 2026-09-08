import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// jsdom implements neither of these; both are fire-and-forget in the UI.
Element.prototype.scrollIntoView = function scrollIntoView(): void {
  // no-op
};
globalThis.requestAnimationFrame = (callback: FrameRequestCallback): number => {
  callback(0);
  return 0;
};

afterEach(() => {
  cleanup();
});
