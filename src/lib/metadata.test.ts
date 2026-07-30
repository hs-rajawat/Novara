import { describe, expect, it } from "vitest";
import { NO_FACTS, parseGameFacts } from "@/lib/metadata";

/** Shaped like the real Steam `appdetails` body the backend stores verbatim. */
const steamPayload = JSON.stringify({
  "620": {
    success: true,
    data: {
      name: "Portal 2",
      developers: ["Valve"],
      publishers: ["Valve"],
      genres: [
        { id: "1", description: "Action" },
        { id: "25", description: "Adventure" },
      ],
      categories: [
        { id: 2, description: "Single-player" },
        { id: 9, description: "Co-op" },
        { id: 28, description: "Full controller support" },
        { id: 42, description: "Steam Deck Verified" },
      ],
      platforms: { windows: true, mac: true, linux: false },
      release_date: { coming_soon: false, date: "18 Apr, 2011" },
      supported_languages:
        "English<strong>*</strong>, French, German<br><strong>*</strong>languages with full audio support",
    },
  },
});

describe("parseGameFacts", () => {
  it("returns nothing for absent, blank or malformed payloads", () => {
    expect(parseGameFacts(null)).toBe(NO_FACTS);
    expect(parseGameFacts(undefined)).toBe(NO_FACTS);
    expect(parseGameFacts("")).toBe(NO_FACTS);
    expect(parseGameFacts("{not json")).toBe(NO_FACTS);
    expect(parseGameFacts("[1,2,3]")).toBe(NO_FACTS);
  });

  it("reads facts out of a Steam appdetails payload keyed by app id", () => {
    const facts = parseGameFacts(steamPayload);
    expect(facts.genres).toEqual(["Action", "Adventure"]);
    expect(facts.releaseDate).toBe("18 Apr, 2011");
    expect(facts.platforms).toEqual(["Windows", "macOS"]);
  });

  it("separates controller and Steam Deck entries from the feature list", () => {
    const facts = parseGameFacts(steamPayload);
    expect(facts.controllerSupport).toBe("Full controller support");
    expect(facts.steamDeck).toBe("Steam Deck Verified");
    expect(facts.features).toEqual(["Single-player", "Co-op"]);
  });

  it("turns the supported_languages display string into a list", () => {
    // Markup, the audio footnote after <br>, and the asterisks must all go.
    expect(parseGameFacts(steamPayload).languages).toEqual([
      "English",
      "French",
      "German",
    ]);
  });

  it("reads a flat payload, and plain string lists", () => {
    const facts = parseGameFacts(
      JSON.stringify({ genres: ["Puzzle"], engine: "Source 2" })
    );
    expect(facts.genres).toEqual(["Puzzle"]);
    expect(facts.engine).toBe("Source 2");
  });

  it("omits fields the provider did not supply", () => {
    const facts = parseGameFacts(JSON.stringify({ data: { name: "Unknown Game" } }));
    expect(facts.genres).toEqual([]);
    expect(facts.languages).toEqual([]);
    expect(facts.controllerSupport).toBeNull();
    expect(facts.steamDeck).toBeNull();
    expect(facts.engine).toBeNull();
    expect(facts.releaseDate).toBeNull();
    expect(facts.platforms).toEqual([]);
  });
});
