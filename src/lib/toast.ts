// A local notification channel, alongside the backend event bus.
//
// `ToastContainer` previously rendered only toasts derived from backend
// `AppEvent`s, which meant a failure originating in the frontend — a rejected
// `invoke`, a render error — had nowhere to go. This is the missing half: a
// tiny publish/subscribe channel the UI can push to directly.
//
// Deliberately not zustand: this is fire-and-forget messaging with no state
// worth persisting or selecting against, and keeping it dependency-free means
// `reportError` can be called from anywhere, including module scope and error
// boundaries, without touching React.

import { errorMessage } from "@/lib/errors";

export type ToastLevel = "info" | "success" | "warning" | "error";

export interface LocalToast {
  message: string;
  level: ToastLevel;
}

type Listener = (toast: LocalToast) => void;

const listeners = new Set<Listener>();

/** Subscribe to locally raised toasts. Returns an unsubscribe function. */
export function subscribeToasts(fn: Listener): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

export function pushToast(toast: LocalToast): void {
  for (const fn of listeners) fn(toast);
}

export function notify(message: string, level: ToastLevel = "info"): void {
  pushToast({ message, level });
}

/**
 * Surface a caught error to the user, and keep the raw value in the console
 * for debugging.
 *
 * `action` reads as "Could not <action>" — e.g. `reportError(e, "launch the
 * game")`. Use this instead of an empty `catch {}`: a silent failure is
 * indistinguishable from success from the user's side, which is precisely how
 * several of the bugs in this codebase survived.
 */
export function reportError(err: unknown, action?: string): void {
  // Kept for developer diagnosis; it is no longer the *only* channel.
  console.error(action ? `[${action}]` : "[error]", err);
  pushToast({ message: errorMessage(err, action), level: "error" });
}
