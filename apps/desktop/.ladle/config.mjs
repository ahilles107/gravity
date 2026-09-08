/** @type {import('@ladle/react').UserConfig} */
export default {
  stories: "src/**/*.stories.{ts,tsx}",
  // The app's vite.config.ts pins a strict dev port and a sourcemap-upload
  // plugin, neither of which belongs in the component workbench.
  viteConfig: ".ladle/vite.config.ts",
  // The app is dark only, so the light/dark toggle has nothing to switch to.
  addons: { theme: { enabled: false } },
};
