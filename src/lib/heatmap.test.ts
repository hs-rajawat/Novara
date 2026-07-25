import { describe, expect, it } from "vitest";

import { buildHeatmap, localDayKey, longestStreak, nextDay } from "./heatmap";
import type { HeatmapCell } from "@/types";

const OFFSET_MINUTES = -new Date().getTimezoneOffset();

/**
 * These tests are only meaningful away from UTC: at +00:00 a local day key and a
 * UTC day key coincide, so every assertion below would pass even with the bug
 * present. `vitest.config.ts` pins TZ to Asia/Kolkata (+05:30); if that stops
 * taking effect this fails loudly rather than going quiet.
 */
describe("timezone precondition", () => {
  it("runs in a non-UTC timezone, or the boundary tests prove nothing", () => {
    expect(OFFSET_MINUTES).not.toBe(0);
  });

  it("uses a half-hour offset, exercising more than whole-hour arithmetic", () => {
    expect(Math.abs(OFFSET_MINUTES) % 60).toBe(30);
  });
});

describe("localDayKey", () => {
  it("returns the local calendar day, not the UTC one", () => {
    // 00:30 local. East of UTC this instant is still the *previous* day in UTC,
    // which is exactly what the old `toISOString().slice(0, 10)` returned.
    const justAfterLocalMidnight = new Date(2026, 2, 8, 0, 30, 0);

    expect(localDayKey(justAfterLocalMidnight)).toBe("2026-03-08");
    expect(justAfterLocalMidnight.toISOString().slice(0, 10)).toBe("2026-03-07");
    expect(localDayKey(justAfterLocalMidnight)).not.toBe(
      justAfterLocalMidnight.toISOString().slice(0, 10)
    );
  });

  it("is stable across a whole local day, including both midnights", () => {
    for (const [h, m] of [
      [0, 0],
      [0, 1],
      [12, 0],
      [23, 58],
      [23, 59],
    ] as const) {
      const d = new Date(2026, 6, 25, h, m, 0);
      expect(localDayKey(d)).toBe("2026-07-25");
    }
  });

  it("rolls over exactly at local midnight, not at UTC midnight", () => {
    const lastMoment = new Date(2026, 6, 25, 23, 59, 59);
    const firstMoment = new Date(2026, 6, 26, 0, 0, 0);
    expect(localDayKey(lastMoment)).toBe("2026-07-25");
    expect(localDayKey(firstMoment)).toBe("2026-07-26");
  });

  it("zero-pads months and days", () => {
    expect(localDayKey(new Date(2026, 0, 5, 12, 0, 0))).toBe("2026-01-05");
  });

  it("handles a leap day", () => {
    expect(localDayKey(new Date(2024, 1, 29, 0, 30, 0))).toBe("2024-02-29");
  });
});

describe("buildHeatmap", () => {
  const cell = (day: string, seconds: number): HeatmapCell => ({ day, seconds });

  it("places a day's activity on that local day", () => {
    const today = new Date(2026, 6, 25, 12, 0, 0);
    const weeks = buildHeatmap([cell("2026-07-25", 3600)], 365, today);

    const placed = weeks.flatMap((w) => w.days).filter((c) => c !== null);
    expect(placed).toHaveLength(1);
    expect(placed[0]).toEqual({ day: "2026-07-25", seconds: 3600 });
  });

  /// The regression: backend keys are local days, and the grid must look them up
  /// with local keys too. Keyed by UTC, today's cell landed on the wrong square.
  it("finds today's cell rather than dropping it a day", () => {
    const today = new Date(2026, 6, 25, 0, 30, 0);
    const weeks = buildHeatmap([cell(localDayKey(today), 60)], 365, today);

    const lastWeek = weeks[weeks.length - 1];
    const lastPopulated = [...lastWeek.days].reverse().find((c) => c !== null);
    expect(lastPopulated).toEqual({ day: "2026-07-25", seconds: 60 });
  });

  it("ends on today and starts on a Sunday", () => {
    const today = new Date(2026, 6, 25, 12, 0, 0); // a Saturday
    const weeks = buildHeatmap([], 365, today);
    expect(weeks[0].firstDate.getDay()).toBe(0);
    expect(weeks.length).toBeGreaterThanOrEqual(53);
  });

  it("ignores activity outside the window", () => {
    const today = new Date(2026, 6, 25, 12, 0, 0);
    const weeks = buildHeatmap([cell("2020-01-01", 9999)], 365, today);
    expect(weeks.flatMap((w) => w.days).filter((c) => c !== null)).toHaveLength(0);
  });

  it("produces a grid of whole weeks", () => {
    const weeks = buildHeatmap([], 365, new Date(2026, 6, 25, 12, 0, 0));
    for (const week of weeks) {
      expect(week.days.length).toBeLessThanOrEqual(7);
    }
    expect(weeks.slice(0, -1).every((w) => w.days.length === 7)).toBe(true);
  });

  /// A DST transition must not drop or duplicate a column. Asia/Kolkata has no
  /// DST, so this asserts the arithmetic survives a span containing what would be
  /// a transition in other zones, using date-component stepping rather than
  /// fixed-millisecond arithmetic.
  it("spans a spring-forward date without losing a day", () => {
    const today = new Date(2026, 2, 15, 12, 0, 0); // mid-March
    const weeks = buildHeatmap([cell("2026-03-08", 120)], 365, today);
    const placed = weeks.flatMap((w) => w.days).filter((c) => c !== null);
    expect(placed).toEqual([{ day: "2026-03-08", seconds: 120 }]);
  });
});

describe("longestStreak", () => {
  it("counts consecutive days", () => {
    expect(longestStreak(["2026-07-01", "2026-07-02", "2026-07-03"])).toBe(3);
  });

  it("resets on a gap", () => {
    expect(
      longestStreak(["2026-07-01", "2026-07-02", "2026-07-05", "2026-07-06"])
    ).toBe(2);
  });

  it("crosses a month boundary", () => {
    expect(longestStreak(["2026-06-29", "2026-06-30", "2026-07-01"])).toBe(3);
  });

  it("crosses a year boundary", () => {
    expect(longestStreak(["2025-12-31", "2026-01-01"])).toBe(2);
  });

  it("crosses a leap day", () => {
    expect(longestStreak(["2024-02-28", "2024-02-29", "2024-03-01"])).toBe(3);
  });

  it("does not inflate a streak from duplicate days", () => {
    expect(longestStreak(["2026-07-01", "2026-07-01", "2026-07-01"])).toBe(1);
  });

  it("is order independent", () => {
    expect(longestStreak(["2026-07-03", "2026-07-01", "2026-07-02"])).toBe(3);
  });

  it("is zero for no activity", () => {
    expect(longestStreak([])).toBe(0);
  });
});

describe("nextDay", () => {
  it("steps across month, year and leap boundaries", () => {
    expect(nextDay("2026-07-25")).toBe("2026-07-26");
    expect(nextDay("2026-06-30")).toBe("2026-07-01");
    expect(nextDay("2025-12-31")).toBe("2026-01-01");
    expect(nextDay("2024-02-28")).toBe("2024-02-29");
    expect(nextDay("2023-02-28")).toBe("2023-03-01");
  });
});
