import posthog from "@posthog/rollup-plugin";
import { defineConfig } from "vite";
import type { PluginOption } from "vite";
import react from "@vitejs/plugin-react";

// Conductor allocates ten ports per workspace starting at CONDUCTOR_PORT, so
// parallel workspaces can each serve the frontend without colliding on 1420.
const port = Number(process.env["CONDUCTOR_PORT"] ?? 1420);

function posthogSourceMaps(): readonly PluginOption[] {
  const personalApiKey = process.env["POSTHOG_PERSONAL_API_KEY"]?.trim();
  const projectId = process.env["POSTHOG_PROJECT_ID"]?.trim();
  if (!personalApiKey || !projectId) {
    return [];
  }
  return [
    posthog({
      personalApiKey,
      projectId,
      host: process.env["POSTHOG_HOST"] ?? "https://eu.i.posthog.com",
      sourcemaps: {
        enabled: true,
        releaseName: "gravity-desktop",
        releaseVersion: process.env["POSTHOG_RELEASE_VERSION"],
        deleteAfterUpload: true,
      },
    }),
  ];
}

export default defineConfig({
  plugins: [react(), ...posthogSourceMaps()],
  clearScreen: false,
  server: {
    port,
    strictPort: true,
  },
  build: {
    target: "es2021",
  },
});
