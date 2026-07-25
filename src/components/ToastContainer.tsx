import { useCallback, useEffect, useState } from "react";
import { onEvent } from "@/lib/ipc";
import { subscribeToasts } from "@/lib/toast";
import type { AppEvent } from "@/types";
import { Icon, type IconName } from "@/components/Icon";

type Level = "info" | "success" | "warning" | "error";

interface Toast {
  id: number;
  message: string;
  level: Level;
}

const LEVEL_ICON: Record<Level, IconName> = {
  info: "info",
  success: "check-circle",
  warning: "alert-triangle",
  error: "alert-circle",
};

let _nextId = 0;

function toastFromEvent(ev: AppEvent): Toast | null {
  switch (ev.type) {
    case "scan_finished": {
      const added = ev.added as number;
      const msg =
        added > 0
          ? `Scan complete — ${added} new game${added === 1 ? "" : "s"} added`
          : "Scan complete — library up to date";
      return { id: _nextId++, message: msg, level: "info" };
    }
    case "save_backup_created":
      return { id: _nextId++, message: "Save backup created", level: "success" };
    case "achievement_unlocked":
      return { id: _nextId++, message: "Achievement unlocked!", level: "success" };
    case "notice":
      return {
        id: _nextId++,
        message: String(ev.message),
        level: (ev.level as string) === "error"
          ? "error"
          : (ev.level as string) === "warning"
          ? "warning"
          : "info",
      };
    default:
      return null;
  }
}

const DURATION_MS = 4500;
const MAX_VISIBLE = 5;

export function ToastContainer() {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    // Tracked so unmount does not leave timers pointing at a dead setState.
    const timers = new Set<ReturnType<typeof setTimeout>>();

    const show = (toast: Toast) => {
      setToasts((prev) => [...prev.slice(-(MAX_VISIBLE - 1)), toast]);
      const timer = setTimeout(() => {
        timers.delete(timer);
        dismiss(toast.id);
      }, DURATION_MS);
      timers.add(timer);
    };

    // Backend events.
    onEvent((ev) => {
      const toast = toastFromEvent(ev);
      if (toast) show(toast);
    }).then((fn) => {
      // The component can unmount before this promise settles; without the
      // guard the listener would outlive it and never be detached.
      if (cancelled) fn();
      else unlisten = fn;
    });

    // Locally raised toasts — errors caught in the frontend.
    const unsubscribe = subscribeToasts((t) =>
      show({ id: _nextId++, message: t.message, level: t.level })
    );

    return () => {
      cancelled = true;
      unlisten?.();
      unsubscribe();
      for (const timer of timers) clearTimeout(timer);
    };
  }, [dismiss]);

  if (toasts.length === 0) return null;

  return (
    <div className="toast-container" role="log" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast-${t.level}`} role="status">
          <span className="toast-icon">
            <Icon name={LEVEL_ICON[t.level]} size={16} />
          </span>
          <span className="toast-msg">{t.message}</span>
          <button
            className="toast-close"
            onClick={() => dismiss(t.id)}
            aria-label="Dismiss notification"
          >
            <Icon name="x" size={14} />
          </button>
          <span
            className="toast-progress"
            style={{ animationDuration: `${DURATION_MS}ms` }}
          />
        </div>
      ))}
    </div>
  );
}
