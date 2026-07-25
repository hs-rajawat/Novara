// Coalescing bursts of backend events into a single refresh.
//
// Every `GameUpdated` used to trigger a full `list_games` in the app shell and,
// on the Dashboard, a `dashboard_stats` plus a `heatmap` as well. A first
// artwork fill of a large library emits one event per game, so the UI would
// issue hundreds of full-table queries in quick succession — enough to make the
// window unresponsive while the fill ran, and enough for the event bus's
// 256-slot capacity to overflow so the forwarder lagged and dropped events.
//
// The backend now emits one event per game rather than per asset, and this is
// the other half: the frontend waits for the burst to settle before refreshing.

/**
 * Wrap `fn` so rapid calls collapse into one trailing invocation.
 *
 * Trailing rather than leading, because the interesting state is the one after
 * the burst finishes, not the one that started it. The returned function also
 * exposes `cancel` so an unmounting component can drop a pending call instead of
 * setting state on a dead component.
 */
export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  waitMs: number
): ((...args: A) => void) & { cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | null = null;

  const wrapped = (...args: A) => {
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      fn(...args);
    }, waitMs);
  };

  wrapped.cancel = () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  };

  return wrapped;
}

/**
 * How long to wait for an event burst to settle.
 *
 * Short enough that a single user-driven change (toggling a favourite) still
 * feels immediate, long enough that a fill emitting events every few
 * milliseconds collapses into one refresh.
 */
export const REFRESH_DEBOUNCE_MS = 300;
