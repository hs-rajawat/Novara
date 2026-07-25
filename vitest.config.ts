import { defineConfig } from "vitest/config";
import { fileURLToPath, URL } from "node:url";

// Kept separate from vite.config.ts on purpose: the app build config is
// typed against Vite's own `defineConfig`, which has no `test` key, and the
// Tauri dev-server settings there are irrelevant to tests.
//
// `environment: "node"` is deliberate. The units worth testing first are
// pure — the library filter/sort selector, heatmap date arithmetic, display
// formatters — and none of them need a DOM. Add jsdom only when a component
// test actually requires it, rather than paying for it on every run.
export default defineConfig({
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    restoreMocks: true,
  },
});
