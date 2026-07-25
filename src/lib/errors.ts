// Translating backend failures into something a user can act on.
//
// `AppError` has always serialized as `{ code, message }` (see
// src-tauri/src/error.rs), and the whole point of the `code` field is to let
// the frontend react per error class. Until now nothing read it: every
// failure was either swallowed by an empty `catch {}` or sent to
// `console.error`, which in a packaged desktop app nobody ever sees. That is
// why a command that fails on every fresh install (`dashboard_stats`
// decoding NULL) went unnoticed.

/** The wire shape of a Rust `AppError`. */
export interface AppErrorWire {
  code: string;
  message: string;
}

const KNOWN_CODES = new Set([
  "db",
  "migrate",
  "io",
  "serde",
  "not_found",
  "invalid",
  "scanner",
  "metadata",
  "save_mgr",
  "other",
]);

/**
 * Normalize anything thrown by `invoke` into `{ code, message }`.
 *
 * Tauri rejects with whatever the command's error serialized to, so the happy
 * path is already an `AppErrorWire`. Everything else — a panic surfaced as a
 * string, a thrown `Error` from our own frontend code, a plugin error — is
 * mapped to `other` so callers never have to defend against the shape.
 */
export function parseAppError(err: unknown): AppErrorWire {
  if (typeof err === "object" && err !== null) {
    const maybe = err as Partial<AppErrorWire>;
    if (typeof maybe.code === "string" && typeof maybe.message === "string") {
      return {
        code: KNOWN_CODES.has(maybe.code) ? maybe.code : "other",
        message: maybe.message,
      };
    }
    if (err instanceof Error) {
      return { code: "other", message: err.message };
    }
  }
  if (typeof err === "string") {
    return { code: "other", message: err };
  }
  return { code: "other", message: String(err) };
}

/**
 * Per-code guidance. The backend `message` is precise but written for a
 * developer ("database error: no rows returned"), so it is kept as detail
 * while the headline explains the consequence and, where possible, what to do
 * about it.
 */
const CODE_HEADLINE: Record<string, string> = {
  db: "Could not read or write the library database",
  migrate: "The library database could not be upgraded",
  io: "A file or folder could not be accessed",
  serde: "Some stored data could not be read",
  not_found: "That item no longer exists",
  invalid: "That input was not accepted",
  scanner: "The library scan could not finish",
  metadata: "Metadata could not be retrieved",
  save_mgr: "The save operation could not be completed",
  other: "Something went wrong",
};

/** A short, user-facing headline for an error. */
export function describeError(err: unknown): string {
  const { code } = parseAppError(err);
  return CODE_HEADLINE[code] ?? CODE_HEADLINE.other;
}

/**
 * A one-line message combining what the user was doing with why it failed.
 *
 * `action` should read as a noun phrase describing the attempted operation
 * ("launch the game", "load achievements"), because it is rendered as
 * "Could not <action>".
 */
export function errorMessage(err: unknown, action?: string): string {
  const { code, message } = parseAppError(err);
  const headline = action
    ? `Could not ${action}`
    : (CODE_HEADLINE[code] ?? CODE_HEADLINE.other);
  // Avoid "Could not launch the game — launch failed: launch failed".
  return message && message.toLowerCase() !== headline.toLowerCase()
    ? `${headline} — ${message}`
    : headline;
}

/** True when the failure means the target row is simply gone. */
export function isNotFound(err: unknown): boolean {
  return parseAppError(err).code === "not_found";
}
