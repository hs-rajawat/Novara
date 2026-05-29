import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * Convert a stored image path to a URL the webview can load.
 * - HTTP(S) / data: URIs are passed through unchanged.
 * - Local absolute paths are converted via Tauri's asset protocol.
 * - null / undefined returns undefined so callers can show fallbacks.
 */
export function toImgSrc(path: string | null | undefined): string | undefined {
  if (!path) return undefined;
  if (path.startsWith("http") || path.startsWith("data:")) return path;
  return convertFileSrc(path);
}
