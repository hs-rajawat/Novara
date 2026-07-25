import { describe, expect, it } from "vitest";

import {
  describeError,
  errorMessage,
  isNotFound,
  parseAppError,
} from "./errors";

// These functions sit between every failed `invoke` and the user, so their
// job is to never throw and never produce an empty message regardless of what
// was thrown at them.

describe("parseAppError", () => {
  it("passes through a well-formed AppError from the backend", () => {
    expect(parseAppError({ code: "db", message: "database error: locked" }))
      .toEqual({ code: "db", message: "database error: locked" });
  });

  it("normalizes an unrecognized code to 'other' rather than trusting it", () => {
    expect(parseAppError({ code: "wat", message: "boom" })).toEqual({
      code: "other",
      message: "boom",
    });
  });

  it("handles a thrown Error", () => {
    expect(parseAppError(new Error("kaboom"))).toEqual({
      code: "other",
      message: "kaboom",
    });
  });

  it("handles a bare string, which is how panics arrive", () => {
    expect(parseAppError("some panic")).toEqual({
      code: "other",
      message: "some panic",
    });
  });

  it("never throws on values with no useful shape", () => {
    for (const v of [null, undefined, 42, [], {}, Symbol("x")]) {
      const parsed = parseAppError(v);
      expect(parsed.code).toBe("other");
      expect(typeof parsed.message).toBe("string");
    }
  });
});

describe("describeError", () => {
  it("maps each known code to its own headline", () => {
    expect(describeError({ code: "not_found", message: "x" })).toMatch(/no longer exists/i);
    expect(describeError({ code: "save_mgr", message: "x" })).toMatch(/save operation/i);
    expect(describeError({ code: "io", message: "x" })).toMatch(/file or folder/i);
  });

  it("falls back for unknown input", () => {
    expect(describeError(null)).toMatch(/something went wrong/i);
  });
});

describe("errorMessage", () => {
  it("combines the attempted action with the backend detail", () => {
    expect(errorMessage({ code: "db", message: "table is locked" }, "save your notes"))
      .toBe("Could not save your notes — table is locked");
  });

  it("uses the code headline when no action is given", () => {
    expect(errorMessage({ code: "io", message: "access denied" }))
      .toBe("A file or folder could not be accessed — access denied");
  });

  it("does not repeat itself when detail matches the headline", () => {
    expect(errorMessage({ code: "other", message: "Could not launch the game" }, "launch the game"))
      .toBe("Could not launch the game");
  });

  it("always produces a non-empty string", () => {
    expect(errorMessage({ code: "db", message: "" }, "do the thing")).toBe(
      "Could not do the thing"
    );
  });
});

describe("isNotFound", () => {
  it("identifies only the not_found code", () => {
    expect(isNotFound({ code: "not_found", message: "gone" })).toBe(true);
    expect(isNotFound({ code: "db", message: "gone" })).toBe(false);
    expect(isNotFound("gone")).toBe(false);
  });
});
