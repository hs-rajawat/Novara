import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { debounce } from "./debounce";

describe("debounce", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("collapses a burst into a single trailing call", () => {
    const fn = vi.fn();
    const wrapped = debounce(fn, 300);

    // A background artwork fill emits an event per game; this is that shape.
    for (let i = 0; i < 50; i++) wrapped();
    expect(fn).not.toHaveBeenCalled();

    vi.advanceTimersByTime(300);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("passes the arguments from the last call", () => {
    const fn = vi.fn();
    const wrapped = debounce(fn, 100);
    wrapped("first");
    wrapped("last");
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledWith("last");
  });

  it("fires again for a later, separate burst", () => {
    const fn = vi.fn();
    const wrapped = debounce(fn, 100);
    wrapped();
    vi.advanceTimersByTime(100);
    wrapped();
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("does not fire while calls keep arriving inside the window", () => {
    const fn = vi.fn();
    const wrapped = debounce(fn, 100);
    for (let i = 0; i < 5; i++) {
      wrapped();
      vi.advanceTimersByTime(90);
    }
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("cancel drops a pending call, so an unmounted component stays quiet", () => {
    const fn = vi.fn();
    const wrapped = debounce(fn, 100);
    wrapped();
    wrapped.cancel();
    vi.advanceTimersByTime(500);
    expect(fn).not.toHaveBeenCalled();
  });

  it("cancel is safe when nothing is pending", () => {
    const wrapped = debounce(vi.fn(), 100);
    expect(() => wrapped.cancel()).not.toThrow();
  });
});
