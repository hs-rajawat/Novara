import { describe, expect, it } from "vitest";

import { formatBytes, formatPlaytime, formatRelative } from "./format";

// These are the display helpers used by the library, timeline, dashboard and
// save manager. They are pure, so they are the cheapest possible proof that
// the frontend test harness works end to end — and they encode the rounding
// behaviour the UI depends on, which is otherwise easy to change by accident.

describe("formatPlaytime", () => {
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
