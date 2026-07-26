import { expect, test } from "./fixtures";
import type { Page } from "@playwright/test";

/**
 * Real-pointer regression tests for the library GameCard.
 *
 * Batch 8 restructured the card so its navigation anchor is a sibling overlay
 * rather than an ancestor of the quick-action buttons, fixing invalid
 * `<button>`-inside-`<a>` markup. That change was verified by asserting DOM
 * structure and by driving navigation with `element.click()` — which bypasses hit
 * testing entirely — so the verification passed while every real mouse click on a
 * card's cover art was being swallowed by the hover-activated `.quick-actions`
 * scrim layered above the link. Clicking a game silently stopped opening its
 * details page, and nothing caught it until manual QA.
 *
 * # On the interactions used here
 *
 * `locator.click()` / `locator.hover()` are **real pointer input**: Playwright
 * moves the mouse and dispatches trusted browser events at coordinates, going
 * through hit testing exactly as a user does. That is categorically different from
 * `element.click()`, which synthesises a DOM event on a chosen node and therefore
 * cannot detect anything layered above it. `element.click()` is never used in this
 * file — it is what let the regression through.
 *
 * They are preferred over raw `page.mouse` because they also auto-wait for the
 * element to be attached, visible and *stable*. The library re-renders whenever
 * the store reloads (a launch, a scan, an artwork fill), and a hand-rolled
 * measure-then-click sequence raced those re-renders: measuring a card mid
 * `.fade-up` animation produced coordinates that were already stale at click time.
 *
 * # Why this is one test with steps, not five tests
 *
 * The application has a single lifetime per run, and a `Page` obtained through
 * `connectOverCDP` does not survive Playwright's per-test boundary against a
 * WebView2 target: with the handle held in a worker fixture, the app was torn down
 * after the first test and every later test failed with "Target page, context or
 * browser has been closed" — and reconnecting per test failed the same way, with
 * `browser.close()` additionally terminating the app rather than just
 * disconnecting. Grouping the assertions into `test.step()` blocks matches the
 * application's actual lifetime, keeps each phase individually reported, and costs
 * one startup instead of five.
 */

/** Invoke a Tauri command from the page, the way the app itself does. */
async function invoke<T>(page: Page, cmd: string, args: unknown = {}): Promise<T> {
  return page.evaluate(
    ([command, params]) =>
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (c: string, a: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__.invoke(command as string, params),
    [cmd, args] as const
  ) as Promise<T>;
}

async function gotoLibrary(page: Page) {
  // Navigating via the sidebar rather than a URL, so the app's own router runs.
  await page.locator('a[href="/library"]').first().click();
  await expect(page.locator("article.game-card").first()).toBeVisible({ timeout: 20_000 });
}

async function firstCardHref(page: Page): Promise<string> {
  const href = await page
    .locator("article.game-card a.game-card-link")
    .first()
    .getAttribute("href");
  expect(href, "the card must expose a navigation target").not.toBeNull();
  return href!;
}

test("GameCard responds to real pointer input", async ({ page }) => {
  test.slow(); // one app lifetime covering several interaction phases

  await test.step("clicking over the cover art opens Game Details", async () => {
    await gotoLibrary(page);
    const href = await firstCardHref(page);
    const card = page.locator("article.game-card").first();
    const link = card.locator("a.game-card-link");

    // Clicking the link at a point over the cover art, rather than clicking the
    // `.game-cover` element itself.
    //
    // This targets whichever element *should* receive the click, which turns
    // Playwright's actionability guard into the assertion: it refuses to click an
    // element when something else would intercept the event, so if the scrim ever
    // becomes click-receiving again this fails with
    // "<div class="quick-actions"> intercepts pointer events" — naming the culprit
    // precisely. Targeting `.game-cover` instead reported the *link* as an
    // interceptor, which is correct-by-design and not a defect.
    //
    // The offset is a fixed 20px from the link's top-left, which sits on the
    // artwork and well clear of the centred action buttons, so no measurement is
    // needed that a re-render could invalidate.
    await link.click({ position: { x: 20, y: 20 }, timeout: 10_000 });

    await expect.poll(() => new URL(page.url()).pathname, { timeout: 15_000 }).toBe(href);
  });

  await test.step("clicking over the metadata strip opens Game Details", async () => {
    await gotoLibrary(page);
    const href = await firstCardHref(page);
    const card = page.locator("article.game-card").first();
    const link = card.locator("a.game-card-link");

    // The metadata strip is the region the scrim does *not* cover — it was still
    // clickable while the regression was live, which is what localised the fault
    // to the cover area. Measured from the link's own box, so the point is inside
    // the bottom strip regardless of card height.
    const box = await link.boundingBox();
    expect(box, "the card link must be laid out").not.toBeNull();
    await link.click({ position: { x: box!.width / 2, y: box!.height - 20 }, timeout: 10_000 });

    await expect.poll(() => new URL(page.url()).pathname, { timeout: 15_000 }).toBe(href);
  });

  await test.step("clicking Play launches the game instead of navigating", async () => {
    await gotoLibrary(page);
    const href = await firstCardHref(page);
    const gameId = href.split("/").pop()!;
    const card = page.locator("article.game-card").first();
    const play = card.locator("button.qa-play");

    if ((await play.count()) === 0 || (await play.isDisabled())) {
      // Reported rather than asserted: with no launchable game the property is
      // untestable, and a false pass would be worse than a visible gap.
      // eslint-disable-next-line no-console
      console.warn("skipped: the first card has no enabled Play button");
      return;
    }

    // Session count read through the app's own command rather than the database,
    // so the assertion is behavioural and needs no SQLite binding.
    const sessionsBefore = (
      await invoke<unknown[]>(page, "list_sessions", { gameId, limit: 200 })
    ).length;

    // The controls only accept pointer events while the card is hovered, which is
    // also the only way a user can reach them.
    await card.hover();
    await page.waitForTimeout(500);
    await play.click();
    await page.waitForTimeout(2500);

    expect(
      new URL(page.url()).pathname,
      "Play must act on the game, not navigate to its details page"
    ).toBe("/library");

    // A launch is observable either as a recorded session or as a reported error.
    // Silence would mean the click reached nothing — the original bug.
    const sessionsAfter = (
      await invoke<unknown[]>(page, "list_sessions", { gameId, limit: 200 })
    ).length;
    const toasts = await page.locator(".toast-msg").allTextContents();
    expect(
      sessionsAfter > sessionsBefore || toasts.length > 0,
      `clicking Play reached the launch handler (sessions ${sessionsBefore} -> ${sessionsAfter}, toasts ${JSON.stringify(toasts)})`
    ).toBe(true);

    // Close the session this step opened so it does not sit open until shutdown.
    // The row itself is left in place: it records a launch that really happened.
    await invoke(page, "stop_session", { gameId }).catch(() => undefined);
  });

  await test.step("the decorative chevron is inert and does not block the card link", async () => {
    await gotoLibrary(page);
    const href = await firstCardHref(page);
    const card = page.locator("article.game-card").first();
    const chevron = card.locator(".qa-details");

    const attrs = await chevron.evaluate((el) => ({
      tag: el.tagName.toLowerCase(),
      ariaHidden: el.getAttribute("aria-hidden"),
      tabIndex: (el as HTMLElement).tabIndex,
      pointerEvents: getComputedStyle(el).pointerEvents,
    }));
    expect(attrs.tag, "decorative, so not a button").toBe("span");
    expect(attrs.ariaHidden, "hidden from assistive technology").toBe("true");
    expect(attrs.tabIndex, "not a tab stop").toBeLessThanOrEqual(0);
    expect(attrs.pointerEvents, "must not shadow the link beneath it").toBe("none");

    // Clicking at the chevron's position must fall through to the card-wide link.
    //
    // `force: true` skips Playwright's "does anything intercept this element?"
    // check, which would otherwise refuse the click precisely because the chevron
    // has `pointer-events: none`. The click is still real pointer input at the
    // chevron's coordinates — falling through is the behaviour under test.
    await card.hover();
    await page.waitForTimeout(500);
    await chevron.click({ force: true });

    await expect.poll(() => new URL(page.url()).pathname, { timeout: 15_000 }).toBe(href);
  });

  await test.step("the quick-action scrim never intercepts clicks, even while hovered", async () => {
    await gotoLibrary(page);
    const card = page.locator("article.game-card").first();

    // Genuine pointer move to establish :hover, as a user arriving at the card.
    await card.hover();
    // The scrim's opacity is transitioned, so a computed-style read taken
    // immediately returns the mid-animation value. Pointer-events is not
    // transitioned, which is why it reads correctly straight away.
    await page.waitForTimeout(600);

    const styles = await page.evaluate(() => {
      const scrim = getComputedStyle(document.querySelector(".quick-actions")!);
      const button = getComputedStyle(document.querySelector("button.qa-btn")!);
      const chevron = getComputedStyle(document.querySelector(".qa-details")!);
      return {
        scrimOpacity: scrim.opacity,
        scrimPointerEvents: scrim.pointerEvents,
        buttonPointerEvents: button.pointerEvents,
        chevronPointerEvents: chevron.pointerEvents,
      };
    });

    expect(Number(styles.scrimOpacity), "the scrim is revealed on hover").toBeGreaterThan(0.5);
    expect(
      styles.scrimPointerEvents,
      "the scrim must stay transparent to pointer events in every state, or it " +
        "swallows clicks meant for the card-wide link beneath it"
    ).toBe("none");
    expect(
      styles.buttonPointerEvents,
      "its real controls must opt back in once visible"
    ).toBe("auto");
    expect(
      styles.chevronPointerEvents,
      "the decorative chevron must not shadow the link"
    ).toBe("none");
  });
});