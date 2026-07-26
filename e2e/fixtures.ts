import { spawn, type ChildProcess } from "node:child_process";
import { test as base, chromium, type Browser, type Page } from "@playwright/test";

/**
 * Launches the Tauri dev application and attaches Playwright to its WebView2.
 *
 * Playwright cannot *launch* a Tauri app — there is no browser binary to start.
 * What it can do is attach to an existing Chrome DevTools Protocol endpoint, and
 * WebView2 exposes one when given `--remote-debugging-port` through
 * `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`. That is the whole mechanism: spawn the
 * app with that variable set, wait for it to come up, then `connectOverCDP`.
 *
 * # Scoping, and why it is split
 *
 * The **process** is worker-scoped, because starting the app costs 60-90 seconds
 * and paying that per test would make the suite unusable.
 *
 * The **connection** is test-scoped. Holding one `Page` handle across tests in a
 * worker fixture did not work: the fixture tore down after the first test and
 * every later test failed with "Target page, context or browser has been closed"
 * against the stale handle. Reconnecting per test is cheap (a socket, not a
 * process) and removes the shared mutable handle entirely.
 *
 * # Platform limitation
 *
 * This is Windows-only by nature. Only WebView2 speaks CDP; WKWebView (macOS) and
 * WebKitGTK (Linux) do not, which is why Tauri's own recommendation for
 * cross-platform end-to-end testing is WebDriver rather than Playwright. NOVARA is
 * Windows-authoritative, so that limit costs nothing today — but it is the reason
 * to move to `@wdio/tauri-service` if these tests ever need to run on macOS.
 */

const CDP_PORT = 9222;
const CDP_ENDPOINT = `http://127.0.0.1:${CDP_PORT}`;
/** The app logs this once `AppState` is constructed and the window is up. */
const READY_MARKER = "initializing app state";
const STARTUP_TIMEOUT_MS = 240_000;

/** Run a command to completion, ignoring failures. Used for cleanup only. */
function runQuietly(command: string, args: string[]): Promise<void> {
  return new Promise((resolve) => {
    const proc = spawn(command, args, { stdio: "ignore", shell: false });
    proc.on("exit", () => resolve());
    proc.on("error", () => resolve());
  });
}

async function waitForPageTarget(timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${CDP_ENDPOINT}/json`);
      const targets = (await res.json()) as { type: string }[];
      if (targets.some((t) => t.type === "page")) return;
    } catch (e) {
      lastError = e;
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error(
    `no DevTools page target on ${CDP_ENDPOINT} after ${timeoutMs}ms (last error: ${lastError})`
  );
}

/**
 * The CDP connection, cached for the application's lifetime.
 *
 * **`browser.close()` is never called.** Against a `connectOverCDP` connection to
 * WebView2 it does not merely disconnect the client — it terminates the
 * application, so the next test fails with `ECONNREFUSED 127.0.0.1:9222`. That is
 * a genuine Playwright/WebView2 limitation, not a workaround preference: the
 * connection has to outlive every test and is cleaned up when the app process is
 * killed at worker teardown.
 */
let connection: Browser | null = null;

async function appPage(): Promise<Page> {
  if (!connection || !connection.isConnected()) {
    connection = await chromium.connectOverCDP(CDP_ENDPOINT);
  }
  const context = connection.contexts()[0];
  if (!context) throw new Error("connected over CDP but there is no browser context");

  // Pick the app's own page rather than assuming index 0: WebView2 can expose
  // more than one target, and a transient one may already be closing.
  const isAppPage = (p: Page) => p.url().includes("localhost:1420");
  const existing = context.pages().find((p) => isAppPage(p) && !p.isClosed());
  if (existing) return existing;
  return context.waitForEvent("page", { predicate: isAppPage, timeout: 30_000 });
}

/** Wait until nothing answers on the CDP port, so we cannot attach to a corpse. */
async function waitForPortFree(timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      await fetch(`${CDP_ENDPOINT}/json/version`);
    } catch {
      return; // refused: nothing is listening
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(
    `something is still listening on ${CDP_ENDPOINT}; a previous instance did not exit`
  );
}

type AppProcess = { readonly pid: number | undefined };

export const test = base.extend<{ page: Page }, { appProcess: AppProcess }>({
  appProcess: [
    async ({}, use) => {
      // Clear out any previous instance *before* launching.
      //
      // Freeing the dev-server port alone was not enough: a lingering app from an
      // earlier run still held the debugging port, so `connectOverCDP` would
      // attach to a process that was already exiting and every action then failed
      // with "Target page, context or browser has been closed". Attaching to a
      // corpse looks exactly like a product bug, so this has to be deterministic.
      await runQuietly("taskkill", ["/IM", "gamevault.exe", "/F"]);
      await runQuietly("powershell", [
        "-NoProfile",
        "-Command",
        "Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue | " +
          "ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }",
      ]);
      await waitForPortFree(30_000);

      // `npm` resolves to npm.cmd on Windows, and Node refuses to spawn a .cmd
      // directly without a shell since the CVE-2024-27980 hardening (it fails
      // with EINVAL). `shell: true` is required here rather than preferred.
      const proc: ChildProcess = spawn("npm", ["run", "tauri:dev"], {
        cwd: process.cwd(),
        shell: true,
        env: {
          ...process.env,
          WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT} --remote-allow-origins=*`,
        },
        stdio: ["ignore", "pipe", "pipe"],
      });

      let log = "";
      const ready = new Promise<void>((resolve, reject) => {
        const timer = setTimeout(
          () =>
            reject(
              new Error(`app did not start in ${STARTUP_TIMEOUT_MS}ms.\n${log.slice(-2000)}`)
            ),
          STARTUP_TIMEOUT_MS
        );
        const onChunk = (chunk: Buffer) => {
          log += chunk.toString();
          if (log.includes(READY_MARKER)) {
            clearTimeout(timer);
            resolve();
          }
        };
        proc.stdout?.on("data", onChunk);
        // Rust's tracing writes to stderr, which is where the marker appears.
        proc.stderr?.on("data", onChunk);
        proc.on("exit", (code) => {
          clearTimeout(timer);
          reject(new Error(`app exited during startup (code ${code}).\n${log.slice(-2000)}`));
        });
      });

      try {
        await ready;
        await waitForPageTarget(60_000);
        // The window is up but React may still be mounting its first route.
        await new Promise((r) => setTimeout(r, 4000));
        await use({ pid: proc.pid });
      } finally {
        connection = null;
        // Surface the application's own output. Without this, an app that exits
        // or panics mid-test appears only as "Target page, context or browser has
        // been closed", which describes the symptom and hides the cause.
        const exited = proc.exitCode !== null || proc.signalCode !== null;
        if (exited) {
          // eslint-disable-next-line no-console
          console.error(
            `\n[app] the application exited during the run ` +
              `(code ${proc.exitCode}, signal ${proc.signalCode}). Last output:\n` +
              log.slice(-3000)
          );
        }
        // Kill the whole tree: `npm run tauri:dev` spawns cargo, which spawns
        // the app.
        if (proc.pid !== undefined) {
          await runQuietly("taskkill", ["/PID", String(proc.pid), "/T", "/F"]);
        }
        await runQuietly("taskkill", ["/IM", "gamevault.exe", "/F"]);
      }
    },
    { scope: "worker", timeout: STARTUP_TIMEOUT_MS + 60_000 },
  ],

  // Overrides Playwright's built-in `page`, so specs read like ordinary tests.
  page: async ({ appProcess }, use) => {
    void appProcess; // ordering dependency: the app must be running first
    await use(await appPage());
  },
});

export { expect } from "@playwright/test";
