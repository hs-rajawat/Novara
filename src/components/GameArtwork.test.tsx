// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { GameArtwork } from "./GameArtwork";

// http/data sources pass through `toImgSrc` untouched, so these exercise the
// load-state handling without needing Tauri's asset-protocol conversion, which is
// unavailable outside the webview and is not what is under test here.

// The artwork image is invisible until `is-loaded` is applied: `.artwork-img` is
// `opacity: 0` and only that class reveals it. Which means the class is not
// cosmetic — if it is never applied, the user sees an empty artwork card.
//
// It used to be driven solely by React's `onLoad`. For a cached image the browser
// can finish before React attaches the handler, so the event fires with nothing
// listening and the card stays blank until an unrelated re-render happens to win
// the race. That was reproducible in the app: open a game's Details page for
// artwork just seen on the Dashboard, and the cover or hero card would be empty.

const CACHED = "https://artwork.test/cached.jpg";
const IN_FLIGHT = "https://artwork.test/in-flight.jpg";
const BROKEN = "https://artwork.test/broken.jpg";

/**
 * jsdom never fetches images, so `complete` and `naturalWidth` have to be stated
 * for the test to mean anything. Stubbing them per source — and never dispatching
 * a `load` event — reproduces exactly the situation the fix exists for: the
 * element already knows it is decoded, and no event is coming to say so.
 */
const state: Record<string, { complete: boolean; naturalWidth: number }> = {
  [CACHED]: { complete: true, naturalWidth: 600 },
  [IN_FLIGHT]: { complete: false, naturalWidth: 0 },
  // A failed load also reports `complete`; only the decoded size gives it away.
  [BROKEN]: { complete: true, naturalWidth: 0 },
};

const originals = (["complete", "naturalWidth"] as const).map(
  (key) => [key, Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, key)] as const,
);

for (const [key] of originals) {
  Object.defineProperty(HTMLImageElement.prototype, key, {
    configurable: true,
    get(this: HTMLImageElement) {
      return state[this.getAttribute("src") ?? ""]?.[key] ?? (key === "complete" ? false : 0);
    },
  });
}

afterEach(cleanup);

describe("GameArtwork", () => {
  it("reveals an image that was already cached when no load event arrives", () => {
    render(<GameArtwork src={CACHED} title="Dying Light" kind="cover" alt="Cover" />);

    expect(
      screen.getByAltText("Cover").className,
      "a cached image must be revealed without waiting for an event that already fired",
    ).toContain("is-loaded");
  });

  it("keeps an image hidden until it has actually loaded", () => {
    render(<GameArtwork src={IN_FLIGHT} title="Dying Light" kind="cover" alt="Cover" />);

    expect(
      screen.getByAltText("Cover").className,
      "an image still in flight must stay hidden so it can blur in",
    ).not.toContain("is-loaded");
  });

  it("reveals a cached hero, which has no placeholder to fall back to", () => {
    // The worst case of the bug: a cover at least shows initials, a hero shows
    // nothing at all, so the Details page renders an empty artwork card.
    render(<GameArtwork src={CACHED} title="Dying Light" kind="hero" alt="Hero" eager />);

    expect(screen.getByAltText("Hero").className).toContain("is-loaded");
  });

  it("treats a completed load with no decoded pixels as a failure", () => {
    render(<GameArtwork src={BROKEN} title="Dying Light" kind="cover" alt="Cover" />);

    expect(
      screen.queryByAltText("Cover"),
      "a broken image must not be revealed, or the card shows a broken-image icon",
    ).toBeNull();
    expect(
      screen.getByText("DL"),
      "the initials placeholder stands in for a cover that cannot be shown",
    ).toBeTruthy();
  });

  it("resets when a card is recycled for a different game", () => {
    // Cards are reused without unmounting on list re-sort and carousel reuse. The
    // per-source reset and the cached-image correction live in the same effect, and
    // this pins that fixing one did not break the other: as two effects, the reset
    // ran second and blanked a cached image.
    const { rerender } = render(
      <GameArtwork src={CACHED} title="Dying Light" kind="cover" alt="Cover" />,
    );
    expect(screen.getByAltText("Cover").className).toContain("is-loaded");

    rerender(<GameArtwork src={IN_FLIGHT} title="Unravel Two" kind="cover" alt="Cover" />);

    expect(
      screen.getByAltText("Cover").className,
      "the previous game's loaded state must not carry over to a new source",
    ).not.toContain("is-loaded");
  });

  it("recovers when a recycled card moves from a broken source to a cached one", () => {
    const { rerender } = render(
      <GameArtwork src={BROKEN} title="Dying Light" kind="cover" alt="Cover" />,
    );
    expect(screen.queryByAltText("Cover")).toBeNull();

    rerender(<GameArtwork src={CACHED} title="Dying Light" kind="cover" alt="Cover" />);

    expect(
      screen.getByAltText("Cover").className,
      "an errored source must not leave the card permanently blank once artwork arrives",
    ).toContain("is-loaded");
  });

  it("renders no image element at all when there is no artwork path", () => {
    render(<GameArtwork src={null} title="Dying Light" kind="hero" alt="Hero" />);
    expect(screen.queryByAltText("Hero")).toBeNull();
  });
});
