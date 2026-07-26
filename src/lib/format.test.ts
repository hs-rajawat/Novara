import { describe, expect, it } from "vitest";

import { formatBytes, formatPlaytime, formatRelative } from "./format";

// These are the display helpers used by the library, timeline, dashboard and
// save manager. They are pure, so they are the cheapest possible proof that
// the frontend test harness works end to end — and they encode the rounding
// behaviour the UI depends on, which is otherwise easy to change by accident.

describe("formatPlaytime", () => {
  // Sub-minute playtime used to round *up* to a minute, so 41 seconds of recorded
  // play was displayed as "1m" — overstating it, and contradicting the game's own
  // state, since 41 seconds is below the threshold at which NOVARA classifies a
  // game as played. The library showed "1m" next to "Unplayed".
  it("renders sub-minute durations in seconds rather than rounding up to a minute", () => {
    expect(formatPlaytime(41)).toBe("41s");
    expect(formatPlaytime(1)).toBe("1s");
    expect(formatPlaytime(59)).toBe("59s");
  });

  it("switches to minutes exactly at the threshold NOVARA counts as played", () => {
    expect(formatPlaytime(59)).toBe("59s");
    expect(formatPlaytime(60)).toBe("1m");
  });

  it("never appears to shrink as playtime grows", () => {
    // Monotonicity across both band boundaries: parsing back to seconds must be
    // non-decreasing, or a longer session could display as less time.
    const toSeconds = (s: string) =>
      s.endsWith("s")
        ? Number(s.slice(0, -1))
        : s.endsWith("m")
          ? Number(s.slice(0, -1)) * 60
          : Number(s.slice(0, -1)) * 3600;

    let previous = 0;
    for (let seconds = 1; seconds <= 7200; seconds += 1) {
      const shown = toSeconds(formatPlaytime(seconds));
      expect(shown, `${seconds}s renders as ${formatPlaytime(seconds)}`).toBeGreaterThanOrEqual(
        previous
      );
      previous = shown;
    }
  });

  it("renders sub-hour durations in minutes", () => {
    expect(formatPlaytime(90)).toBe("2m");
    expect(formatPlaytime(1800)).toBe("30m");
  });

  it("renders one decimal below ten hours and whole hours above", () => {
    expect(formatPlaytime(3600)).toBe("1.0h");
    expect(formatPlaytime(5400)).toBe("1.5h");
    expect(formatPlaytime(36000)).toBe("10h");
    expect(formatPlaytime(37800)).toBe("11h");
  });

  it("treats zero, negative and missing values as 0h rather than throwing", () => {
    expect(formatPlaytime(0)).toBe("0h");
    expect(formatPlaytime(-1)).toBe("0h");
    expect(formatPlaytime(undefined as unknown as number)).toBe("0h");
  });
});

describe("formatBytes", () => {
  it("scales through units", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 * 1024 * 1024)).toBe("1.0 GB");
  });

  it("drops the decimal at three significant digits to keep columns narrow", () => {
    expect(formatBytes(100 * 1024)).toBe("100 KB");
    expect(formatBytes(99 * 1024)).toBe("99.0 KB");
  });

  it("renders an em dash for absent sizes", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(undefined)).toBe("—");
    expect(formatBytes(0)).toBe("—");
  });
});

describe("formatRelative", () => {
  it("renders an em dash for absent timestamps", () => {
    expect(formatRelative(null)).toBe("—");
    expect(formatRelative(undefined)).toBe("—");
  });

  it("falls back to the raw value rather than throwing on unparseable input", () => {
    expect(formatRelative("not a date")).toBe("not a date");
  });
});
