import { useCallback, useEffect, useState } from "react";
import { onEvent } from "@/lib/ipc";
import type { AppEvent } from "@/types";

interface Toast {
  id: number;
  message: string;
  level: "info" | "success" | "warning" | "error";
}

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

const DURATION_MS = 4000;
const MAX_VISIBLE = 5;

export function ToastContainer() {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    onEvent((ev) => {
      const toast = toastFromEvent(ev);
      if (!toast) return;
      setToasts((prev) => [...prev.slice(-(MAX_VISIBLE - 1)), toast]);
      setTimeout(() => dismiss(toast.id), DURATION_MS);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [dismiss]);

  if (toasts.length === 0) return null;

  return (
    <div className="toast-container" role="log" aria-live="polite">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`toast toast-${t.level}`}
          onClick={() => dismiss(t.id)}
          role="status"
        >
          {t.message}
        </div>
      ))}
    </div>
  );
}
