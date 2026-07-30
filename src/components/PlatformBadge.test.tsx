// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";

import { PlatformBadge } from "./PlatformBadge";

// The source badge is the first thing on the Game Details hero that says where a
// game lives (DESIGN.md §21.6), and its marks are hand-drawn glyphs rather than
// vendor logo files. These tests pin the two things that would silently break
// that: a store losing its mark, and an unknown store rendering nothing at all.

afterEach(cleanup);

/** Every code seeded into the `sources` table (migration 0001). */
const SEEDED: [code: string, label: string][] = [
  ["steam", "Steam"],
  ["epic", "Epic Games"],
  ["gog", "GOG"],
  ["xbox", "Xbox App"],
  ["ubisoft", "Ubisoft Connect"],
  ["battle", "Battle.net"],
  ["emulator", "Emulator ROM"],
  ["manual", "Manual / Other"],
];

describe("PlatformBadge", () => {
  it("renders nothing without both a code and a label", () => {
    const { container } = render(<PlatformBadge code={null} label="Steam" />);
    expect(container.firstChild).toBeNull();
    cleanup();
    const second = render(<PlatformBadge code="steam" label={null} />);
    expect(second.container.firstChild).toBeNull();
  });

  it("is a tone dot with no glyph by default", () => {
    const { container } = render(<PlatformBadge code="steam" label="Steam" />);
    const badge = container.firstElementChild as HTMLElement;
    expect(badge.className).toContain("tone-steam");
    expect(badge.className).not.toContain("has-icon");
    expect(badge.querySelector("svg")).toBeNull();
  });

  it.each(SEEDED)("gives %s a launcher mark and its own tone", (code, label) => {
    const { container } = render(<PlatformBadge code={code} label={label} withIcon />);
    const badge = container.firstElementChild as HTMLElement;
    expect(badge.className).toContain(`tone-${code}`);
    expect(badge.className).toContain("has-icon");
    expect(badge.querySelector("svg")).not.toBeNull();
    expect(badge.textContent).toBe(label);
  });

  it("falls back to a generic mark and neutral tone for an unknown store", () => {
    const { container } = render(
      <PlatformBadge code="futurestore" label="Future Store" withIcon />
    );
    const badge = container.firstElementChild as HTMLElement;
    expect(badge.className).toContain("tone-default");
    expect(badge.querySelector("svg")).not.toBeNull();
    expect(badge.textContent).toBe("Future Store");
  });
});
