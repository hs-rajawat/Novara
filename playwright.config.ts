import { defineConfig } from "@playwright/test";

/**
 * Pointer-level end-to-end tests, run against a real Tauri dev instance.
 *
 * There is no `projects` block and no browser download: these tests attach to the
 * application's own WebView2 over the Chrome DevTools Protocol rather than
 * launching a browser, so `npx playwright install` is not required.
 */
export default defineConfig({
  testDir: "./e2e",
  // Starting the app takes ~30-60s, and the fixture is worker-scoped, so one
  // worker keeps that cost to a single startup for the whole file.
  workers: 1,
  fullyParallel: false,
  // Each test drives a real application; failures here are usually real, so a
  // retry mostly costs time. One retry absorbs genuine flake (app startup,
  // animation timing) without hiding a consistent break.
  retries: process.env.CI ? 1 : 0,
  timeout: 90_000,
  expect: { timeout: 10_000 },
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : [["list"]],
  use: {
    // Traces make a hit-testing failure diagnosable without re-running by hand,
    // which is exactly what was missing when this regression slipped through.
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
});
