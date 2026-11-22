// Vite config for the Anti-Power Usage panel bundled into the plugin

import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("./plugin-ui", import.meta.url));
const outDir = fileURLToPath(new URL("../plugin/sidecars/panel/web", import.meta.url));

export default defineConfig({
  root,
  plugins: [vue()],
  build: {
    outDir,
    emptyOutDir: true,
    // The sidecar serves a fixed set of paths, so keep file names stable and in one bundle
    assetsInlineLimit: 100_000,
    cssCodeSplit: false,
    modulePreload: false,
    rolldownOptions: {
      // The sidecar page loads a classic script inside Antigravity's plugin view
      input: fileURLToPath(new URL("./plugin-ui/src/main.ts", import.meta.url)),
      output: {
        format: "iife",
        entryFileNames: "app.js",
        assetFileNames: "app[extname]",
        codeSplitting: false,
      },
    },
  },
});
