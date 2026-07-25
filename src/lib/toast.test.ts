import { beforeEach, describe, expect, it, vi } from "vitest";

import { notify, pushToast, reportError, subscribeToasts, type LocalToast } from "./toast";

describe("local toast channel", () => {
  let seen: LocalToast[];

  beforeEach(() => {
    seen = [];
    // reportError also logs; silence it so test output stays readable while
    // still asserting the toast is raised.
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  it("delivers to every subscriber", () => {
    const a: LocalToast[] = [];
    const b: LocalToast[] = [];
    const offA = subscribeToasts((t) => a.push(t));
    const offB = subscribeToasts((t) => b.push(t));

    pushToast({ message: "hello", level: "info" });

    expect(a).toEqual([{ message: "hello", level: "info" }]);
    expect(b).toEqual([{ message: "hello", level: "info" }]);
    offA();
    offB();
  });

  it("stops delivering after unsubscribe, so unmounted components go quiet", () => {
    const off = subscribeToasts((t) => seen.push(t));
    notify("first");
    off();
    notify("second");
    expect(seen.map((t) => t.message)).toEqual(["first"]);
  });

  it("defaults notify to info level", () => {
    const off = subscribeToasts((t) => seen.push(t));
    notify("plain");
    off();
    expect(seen[0].level).toBe("info");
  });

  it("reportError raises an error-level toast with the action context", () => {
    const off = subscribeToasts((t) => seen.push(t));
    reportError({ code: "db", message: "table is locked" }, "save your notes");
    off();

    expect(seen).toHaveLength(1);
    expect(seen[0].level).toBe("error");
    expect(seen[0].message).toBe("Could not save your notes — table is locked");
  });

  it("reportError still keeps the raw value in the console for diagnosis", () => {
    const off = subscribeToasts((t) => seen.push(t));
    const raw = { code: "io", message: "denied" };
    reportError(raw, "open the folder");
    off();
    expect(console.error).toHaveBeenCalledWith("[open the folder]", raw);
  });

  it("survives an error thrown by an unrelated subscriber's message", () => {
    const off = subscribeToasts((t) => seen.push(t));
    reportError(undefined, "do a thing");
    off();
    expect(seen[0].message).toBe("Could not do a thing — undefined");
  });
});
