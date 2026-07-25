import { useCallback, useEffect, useRef } from "react";

import { Icon } from "@/components/Icon";

/**
 * A generic overlay panel.
 *
 * DESIGN.md §22 specifies overlay panels with a focus trap and Escape/backdrop
 * dismissal, and §28 forbids browser-default controls — but destructive
 * confirmations used `window.confirm()`, which breaks the visual identity at
 * exactly the moment it matters most and offers no keyboard-trap behaviour.
 *
 * Deliberately generic: this is a panel that renders whatever it is given, not a
 * confirmation dialog. Confirmations are one composition of it
 * ([`ConfirmDialog`](./ConfirmDialog)); a future settings sheet or artwork picker
 * is another.
 *
 * Accessibility behaviour, all of which `window.confirm` gave for free and a
 * hand-rolled overlay must earn back:
 *   * `role="dialog"` with `aria-modal` and a label wired to the title.
 *   * Focus moves into the panel on open and returns to the previously focused
 *     element on close, so keyboard users are not dropped at the top of the page.
 *   * Tab and Shift+Tab cycle within the panel.
 *   * Escape and a backdrop click dismiss.
 */
export interface ModalProps {
  open: boolean;
  title: string;
  description?: string;
  onClose: () => void;
  children?: React.ReactNode;
  footer?: React.ReactNode;
  /** Rendered in the header; defaults to no icon. */
  icon?: React.ComponentProps<typeof Icon>["name"];
  /** Adds the danger accent for destructive actions. */
  tone?: "default" | "danger";
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function Modal({
  open,
  title,
  description,
  onClose,
  children,
  footer,
  icon,
  tone = "default",
}: ModalProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const restoreFocusTo = useRef<HTMLElement | null>(null);
  const titleId = useRef(`modal-title-${Math.random().toString(36).slice(2)}`);

  const focusables = useCallback((): HTMLElement[] => {
    const panel = panelRef.current;
    if (!panel) return [];
    // Filtered by attributes rather than by layout. An `offsetParent !== null`
    // visibility check is the usual trick, but it depends on a rendered layout —
    // it reports null for position:fixed elements and for every element under a
    // test renderer, which silently emptied this list and disabled the focus
    // trap. Nothing here hides controls with CSS, so the attributes are enough.
    return Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      (el) =>
        !el.hasAttribute("hidden") &&
        el.getAttribute("aria-hidden") !== "true" &&
        !(el as HTMLButtonElement).disabled
    );
  }, []);

  // Remember where focus came from, and put it back on close.
  useEffect(() => {
    if (!open) return;
    restoreFocusTo.current = document.activeElement as HTMLElement | null;
    // Focus the first control rather than the panel itself, so Enter acts on the
    // primary action immediately.
    const first = focusables()[0] ?? panelRef.current;
    first?.focus();
    return () => {
      restoreFocusTo.current?.focus?.();
    };
  }, [open, focusables]);

  useEffect(() => {
    if (!open) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
        return;
      }
      if (e.key !== "Tab") return;
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      // Wrap at both ends so focus cannot escape the panel.
      if (e.shiftKey && (active === first || !panelRef.current?.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [open, onClose, focusables]);

  if (!open) return null;

  return (
    <div
      className="modal-backdrop"
      // A click on the backdrop dismisses; a click inside the panel must not.
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className={`modal-panel${tone === "danger" ? " is-danger" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId.current}
        ref={panelRef}
        tabIndex={-1}
      >
        <div className="modal-head">
          {icon && (
            <span className="modal-icon">
              <Icon name={icon} size={18} />
            </span>
          )}
          <h3 id={titleId.current}>{title}</h3>
          <button className="modal-close" onClick={onClose} aria-label="Close dialog">
            <Icon name="x" size={15} />
          </button>
        </div>
        {description && <p className="modal-description">{description}</p>}
        {children}
        {footer && <div className="modal-actions">{footer}</div>}
      </div>
    </div>
  );
}
