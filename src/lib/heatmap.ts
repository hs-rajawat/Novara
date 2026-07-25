// Heatmap date arithmetic.
//
// Extracted from the Analytics page so the calendar logic can be tested without
// rendering SVG, because it was wrong in a way no visual inspection would catch.
//
// # Project-wide convention
//
// This module is one application of a NOVARA-wide rule, stated in full in
// `src-tauri/src/commands/analytics.rs`: every calendar-based aggregation buckets
// by the user's **local** calendar day unless a feature explicitly documents an
// exception. Any future day-keyed feature (a weekly summary, "played this
// month") should reuse `localDayKey` below rather than reintroducing a
// `toISOString()`-based key.
//
// # The bug this module exists to prevent
//
// The grid was built from *local* midnights (`setHours(0,0,0,0)`) and then each
// day was keyed with `date.toISOString().slice(0, 10)`, which converts to UTC.
// For any timezone east of UTC, local midnight falls on the previous UTC day, so
// every cell was labelled with the wrong date — the entire grid, and the
// longest-streak figure derived from it, shifted by one day. West of UTC the same
// mismatch appears at the other end of the day.
//
// The rule now: **a day is a local calendar day, everywhere.** The backend groups
// sessions with SQLite's `date(started_at, 'localtime')`, and the keys built here
// are formed from local date components. `toISOString` must never be used to
// derive a day key.

import type { HeatmapCell } from "@/types";

export interface HeatmapWeek {
  days: (HeatmapCell | null)[];
  firstDate: Date;
}

/**
 * The `YYYY-MM-DD` key for the local calendar day a `Date` falls on.
 *
 * Uses local components deliberately. `toISOString().slice(0, 10)` is the same
 * shape and silently wrong away from UTC.
 */
export function localDayKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

/**
 * Build the week columns for the heatmap grid, covering `days` days up to and
 * including today, padded back to the preceding Sunday so rows line up.
 */
export function buildHeatmap(cells: HeatmapCell[], days = 365, today = new Date()): HeatmapWeek[] {
  const byDay = new Map(cells.map((c) => [c.day, c]));

  const end = new Date(today);
  end.setHours(0, 0, 0, 0);
  const start = new Date(end);
  start.setDate(start.getDate() - (days - 1));
  // Pad back to Sunday so every column is a full week.
  start.setDate(start.getDate() - start.getDay());

  const grid: { date: Date; cell: HeatmapCell | null }[] = [];
  for (const cursor = new Date(start); cursor <= end; cursor.setDate(cursor.getDate() + 1)) {
    grid.push({
      date: new Date(cursor),
      cell: byDay.get(localDayKey(cursor)) ?? null,
    });
  }

  const weeks: HeatmapWeek[] = [];
  for (let i = 0; i < grid.length; i += 7) {
    const chunk = grid.slice(i, i + 7);
    weeks.push({
      days: chunk.map((d) => d.cell),
      firstDate: chunk[0].date,
    });
  }
  return weeks;
}

/**
 * The longest run of consecutive days in a set of `YYYY-MM-DD` keys.
 *
 * Day arithmetic is done on the date string through UTC, which is safe: shifting
 * a date-only value by one UTC day cannot cross a local boundary because no time
 * of day is involved. Duplicates are collapsed so a repeated day cannot inflate
 * a streak.
 */
export function longestStreak(activeDays: string[]): number {
  const sorted = [...new Set(activeDays)].sort();
  let best = 0;
  let current = 0;
  let previous: string | null = null;
  for (const day of sorted) {
    current = previous !== null && nextDay(previous) === day ? current + 1 : 1;
    if (current > best) best = current;
    previous = day;
  }
  return best;
}

/** The `YYYY-MM-DD` key following `iso`. */
export function nextDay(iso: string): string {
  const d = new Date(`${iso}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + 1);
  return d.toISOString().slice(0, 10);
}
