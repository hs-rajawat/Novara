import { useCallback, useRef, useState } from "react";

import { Modal } from "@/components/Modal";

/**
 * One confirmation dialog, driven by a hook, rather than a bespoke dialog per
 * call site.
 *
 * Replaces `window.confirm()`, which DESIGN.md §28 forbids and which cannot be
 * styled, focus-trapped, or given a meaningful action label. The hook keeps the
 * ergonomics of `confirm` — ask a question, await a boolean — so call sites stay
 * as readable as they were:
 *
 * ```tsx
 * const confirm = useConfirm();
 * if (!(await confirm({ title: "Delete this?", confirmLabel: "Delete" }))) return;
 * ```
 */
export interface ConfirmOptions {
  title: string;
  description?: string;
  /** Defaults to "Confirm". */
  confirmLabel?: string;
  /** Defaults to "Cancel". */
  cancelLabel?: string;
  /** `danger` styles the primary action as destructive. */
  tone?: "default" | "danger";
  icon?: React.ComponentProps<typeof Modal>["icon"];
}

interface Pending extends ConfirmOptions {
  resolve: (confirmed: boolean) => void;
}

export function useConfirm() {
  const [pending, setPending] = useState<Pending | null>(null);
  // Held in a ref so `confirm` is stable and safe to call from effects.
  const pendingRef = useRef<Pending | null>(null);
  pendingRef.current = pending;

  const confirm = useCallback(
    (options: ConfirmOptions) =>
      new Promise<boolean>((resolve) => {
        setPending({ ...options, resolve });
      }),
    []
  );

  const settle = useCallback((confirmed: boolean) => {
    const current = pendingRef.current;
    setPending(null);
    current?.resolve(confirmed);
  }, []);

  const dialog = pending ? (
    <Modal
      open
      title={pending.title}
      description={pending.description}
      icon={pending.icon}
      tone={pending.tone}
      // Escape and backdrop dismissal must resolve the promise, or an awaiting
      // caller would hang forever.
      onClose={() => settle(false)}
      footer={
        <>
          <button className="btn" onClick={() => settle(false)}>
            {pending.cancelLabel ?? "Cancel"}
          </button>
          <button
            className={`btn ${pending.tone === "danger" ? "btn-danger" : "btn-primary"}`}
            onClick={() => settle(true)}
          >
            {pending.confirmLabel ?? "Confirm"}
          </button>
        </>
      }
    />
  ) : null;

  return { confirm, dialog };
}
