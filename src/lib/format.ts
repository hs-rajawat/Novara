import { format, formatDistanceToNowStrict, isToday, isYesterday, parseISO } from "date-fns";

/**
 * Human playtime.
 *
 * Sub-minute time is shown in seconds rather than rounded up to a minute. The
 * rounding version reported "1m" for 41 seconds, which both overstated the number
 * and contradicted the game's state: 41 seconds is below the threshold at which
 * NOVARA considers a game played, so the library showed "1m" next to "Unplayed".
 * Bands are monotonic — seconds below a minute, minutes below an hour, hours
 * above — so a growing total never appears to shrink.
 */
export function formatPlaytime(seconds: number): string {
  if (!seconds || seconds <= 0) return "0h";
  if (seconds < 60) return `${Math.round(seconds)}s`;
  const hours = seconds / 3600;
  if (hours < 1) return `${Math.round(seconds / 60)}m`;
  if (hours < 10) return `${hours.toFixed(1)}h`;
  return `${Math.round(hours)}h`;
}

export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`;
}

export function formatRelative(iso: string | null | undefined): string {
  if (!iso) return "—";
  try {
    return formatDistanceToNowStrict(parseISO(iso), { addSuffix: true });
  } catch {
    return iso;
  }
}

/** "Today" / "Yesterday" / "Thursday, Mar 5" — used by session lists. */
export function formatSessionDay(iso: string): string {
  try {
    const d = parseISO(iso);
    if (isToday(d)) return "Today";
    if (isYesterday(d)) return "Yesterday";
    return format(d, "EEEE, MMM d");
  } catch {
    return iso;
  }
}

/** "3:45 PM" — used alongside formatSessionDay. */
export function formatSessionTime(iso: string): string {
  try {
    return format(parseISO(iso), "p");
  } catch {
    return "";
  }
}
