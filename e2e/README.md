# Pointer-level end-to-end tests

```
npm run test:e2e
```

No browser download is needed. These tests attach to the application's own
WebView2 over the Chrome DevTools Protocol rather than launching a browser, so
`npx playwright install` is not required.

## Why these exist separately from the Vitest suite

The Vitest suite runs under jsdom, which performs **no layout**. It can assert
that the DOM is structured correctly — that no `<button>` is nested inside an
`<a>`, that controls have accessible names — but it cannot answer:

> when the user clicks at this point on screen, which element actually receives it?

That distinction caused a real regression. Batch 8 restructured the game card so
its navigation anchor became a sibling overlay rather than an ancestor of the
quick-action buttons, which fixed invalid markup. The change was verified by
asserting DOM structure and by driving navigation with `element.click()` — which
bypasses hit testing entirely. Both checks passed while every real mouse click on
a card's cover art was being swallowed by the hover-activated `.quick-actions`
scrim layered above the link. Clicking a game stopped opening its details page,
and nothing caught it until manual QA.

`locator.click()` and `locator.hover()` are **real pointer input**: Playwright
moves the mouse and dispatches trusted browser events at coordinates, through hit
testing, as a user does. `element.click()` synthesises a DOM event on a node you
have already chosen, and therefore cannot see anything layered above it. **It is
never used in these tests** — it is what let the regression through.

### Targeting the receiver, not the region

Clicks target the element that *should* receive them — the overlay link — at a
position over the region of interest, rather than targeting `.game-cover` or
`.game-meta`. This turns Playwright's actionability guard into the assertion: it
refuses to click when something else would intercept the event, so reintroducing
the bug fails with

```
<div class="quick-actions">…</div> from <div class="game-cover">…</div> subtree
intercepts pointer events
```

which names the culprit exactly. That failure mode has been verified by
temporarily restoring the bug and observing it.

## When to add a test here

Add one when the property depends on layout or input routing rather than markup:

- overlays and scrims, and anything using `pointer-events` or `z-index` to decide
  what is clickable
- click targets visually inside one element but semantically another
- hover-revealed controls, where the state at press time differs from hover time
- drag, scroll or focus behaviour that depends on real geometry

Anything assertable without geometry belongs in Vitest, which is far faster and
needs no running application.

## Platform limitation

This is **Windows-only**, and not by choice. Only WebView2 exposes CDP; WKWebView
(macOS) and WebKitGTK (Linux) do not, which is why Tauri's own recommendation for
cross-platform end-to-end testing is WebDriver (`@wdio/tauri-service`) rather than
Playwright. NOVARA is Windows-authoritative, so this costs nothing today — but if
these tests ever need to run on macOS, that is the reason to switch.

## WebView2 quirks worth knowing before editing `fixtures.ts`

Each of these was hit during the port; they are recorded so they are not
rediscovered:

- **`browser.close()` terminates the application.** Against a `connectOverCDP`
  connection it does not merely disconnect the client — the app exits and the next
  action fails with `ECONNREFUSED 127.0.0.1:9222`. The connection is therefore
  cached and never closed; it dies with the app process at teardown.
- **A `Page` handle does not survive Playwright's per-test boundary.** Holding one
  in a worker fixture led to the app being torn down after the first test, and
  reconnecting per test failed the same way. The assertions are consequently one
  test with `test.step()` blocks, which also matches the application's real
  lifetime and costs one startup rather than five.
- **`context.newCDPSession()` is best avoided.** Playwright already owns that CDP
  connection; re-entering it to force pseudo-states was unnecessary once hover was
  established with a genuine pointer move.
- **A lingering instance holds the debugging port.** Without killing
  `gamevault.exe` and waiting for the port to be refused before launching,
  `connectOverCDP` attaches to an exiting process and every action fails with
  "Target page, context or browser has been closed" — indistinguishable from a
  product bug.
- **`spawn` cannot start `npm` directly.** Node refuses to spawn a `.cmd` without a
  shell since the CVE-2024-27980 hardening, failing with `EINVAL`, so `shell: true`
  is required.

## Cost and side effects

A run takes roughly 20-30 seconds once Rust is warm, and a few minutes cold
because the app is compiled first. It is not part of `npm test`; run it when
touching interaction layering, and before a release.

Verifying "Play launches instead of navigating" clicks a real Play button, because
there is no other way to establish it. That genuinely launches the game (or its
launcher) and records a `play_sessions` row. The test closes the session it opened
but deliberately keeps the row: it records a launch that really happened, and
deleting real history to tidy a test would be worse than the residue. Expect, per
run, one launched game or launcher process you may need to close.

If the first card's Play button is disabled (the game is missing), that phase
reports a warning and is skipped rather than asserting falsely.
