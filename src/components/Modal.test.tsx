// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { Modal } from "./Modal";

// These cover the behaviour `window.confirm()` provided for free and that a
// hand-rolled overlay has to earn back: dialog semantics, focus management and
// keyboard dismissal.

afterEach(cleanup);

function renderModal(props: Partial<React.ComponentProps<typeof Modal>> = {}) {
  const onClose = vi.fn();
  render(
    <Modal
      open
      title="Delete this thing?"
      description="It cannot be undone."
      onClose={onClose}
      footer={
        <>
          <button>Cancel</button>
          <button>Delete</button>
        </>
      }
      {...props}
    />
  );
  return { onClose };
}

describe("Modal semantics", () => {
  it("renders nothing when closed", () => {
    renderModal({ open: false });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("exposes itself as a modal dialog labelled by its title", () => {
    renderModal();
    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    const labelId = dialog.getAttribute("aria-labelledby");
    expect(labelId).toBeTruthy();
    expect(document.getElementById(labelId!)?.textContent).toBe("Delete this thing?");
  });

  it("gives the close control an accessible name, not just a title", () => {
    renderModal();
    expect(screen.getByRole("button", { name: "Close dialog" })).toBeTruthy();
  });
});

describe("Modal focus management", () => {
  it("moves focus into the panel on open", () => {
    renderModal();
    const dialog = screen.getByRole("dialog");
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it("returns focus to the element that opened it", () => {
    const opener = document.createElement("button");
    opener.textContent = "Open";
    document.body.appendChild(opener);
    opener.focus();
    expect(document.activeElement).toBe(opener);

    const { unmount } = render(
      <Modal open title="Question?" onClose={() => {}} footer={<button>OK</button>} />
    );
    expect(document.activeElement).not.toBe(opener);

    unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  it("wraps Tab from the last control back to the first", () => {
    renderModal();
    const dialog = screen.getByRole("dialog");
    const focusable = Array.from(
      dialog.querySelectorAll<HTMLElement>("button")
    );
    const last = focusable[focusable.length - 1];
    last.focus();

    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(focusable[0]);
  });

  it("wraps Shift+Tab from the first control back to the last", () => {
    renderModal();
    const dialog = screen.getByRole("dialog");
    const focusable = Array.from(dialog.querySelectorAll<HTMLElement>("button"));
    focusable[0].focus();

    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(focusable[focusable.length - 1]);
  });
});

describe("Modal dismissal", () => {
  it("closes on Escape", () => {
    const { onClose } = renderModal();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes on a backdrop click", () => {
    const { onClose } = renderModal();
    const backdrop = screen.getByRole("dialog").parentElement!;
    fireEvent.mouseDown(backdrop);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("does not close when the panel itself is clicked", () => {
    const { onClose } = renderModal();
    fireEvent.mouseDown(screen.getByRole("dialog"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("stops listening for Escape once closed", () => {
    const onClose = vi.fn();
    const { unmount } = render(
      <Modal open title="Q" onClose={onClose} footer={<button>OK</button>} />
    );
    unmount();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
  });
});
