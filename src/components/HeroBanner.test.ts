import { describe, expect, it } from "vitest";

import { pickFeatured } from "./HeroBanner";
import type { Game } from "@/types";

// The reported symptom: RDR2 was played most recently, but the banner kept
// showing THE FINALS — labelled "Continue Playing" — because a favourite that had
// been played outranked the most recently played game.

let counter = 0;

function game(overrides: Partial<Game> = {}): Game {
  counter += 1;
  return {
    id: `g${counter}`,
    title: `Game ${counter}`,
    sort_title: `game ${counter}`,
    description: null,
    release_year: null,
    developer: null,
    publisher: null,
    cover_path: null,
    hero_path: null,
    icon_path: null,
    logo_path: null,
    metadata_json: null,
    metadata_source: null,
    is_favorite: 0,
    is_hidden: 0,
    completion_pct: 0,
    completion_state: "unplayed",
    user_rating: null,
    user_notes: null,
    total_playtime_seconds: 0,
    last_played_at: null,
    added_at: "2026-01-01T00:00:00+00:00",
    updated_at: "2026-01-01T00:00:00+00:00",
    ...overrides,
  } as Game;
}

describe("pickFeatured priority", () => {
  it("prefers the most recently played game over a favourite", () => {
    const favourite = game({
      title: "THE FINALS",
      is_favorite: 1,
      last_played_at: "2026-07-24T04:46:46+00:00",
    });
    const recent = game({
      title: "Red Dead Redemption 2",
      last_played_at: "2026-07-25T13:18:39+00:00",
    });

    const featured = pickFeatured([favourite, recent]);
    expect(featured?.game.title).toBe("Red Dead Redemption 2");
  });

  it("is unaffected by the order games arrive in", () => {
    const favourite = game({
      title: "Favourite",
      is_favorite: 1,
      last_played_at: "2026-07-01T00:00:00+00:00",
    });
    const recent = game({ title: "Recent", last_played_at: "2026-07-20T00:00:00+00:00" });
    expect(pickFeatured([favourite, recent])?.game.title).toBe("Recent");
    expect(pickFeatured([recent, favourite])?.game.title).toBe("Recent");
  });

  it("uses favourite status only to break a tie on recency", () => {
    const sameTime = "2026-07-20T00:00:00+00:00";
    const plain = game({ title: "Plain", last_played_at: sameTime });
    const favourite = game({ title: "Favourite", is_favorite: 1, last_played_at: sameTime });
    expect(pickFeatured([plain, favourite])?.game.title).toBe("Favourite");
  });

  it("ignores games that have never been played when something has", () => {
    const never = game({ title: "Never", is_favorite: 1 });
    const played = game({ title: "Played", last_played_at: "2026-07-20T00:00:00+00:00" });
    expect(pickFeatured([never, played])?.game.title).toBe("Played");
  });
});

describe("pickFeatured labelling", () => {
  it('says "Continue Playing" only for a game that really is in progress', () => {
    const inProgress = game({
      completion_state: "playing",
      last_played_at: "2026-07-25T00:00:00+00:00",
    });
    expect(pickFeatured([inProgress])?.reason).toBe("Continue Playing");
  });

  it('says "Jump Back In" for a recently played game that is not in progress', () => {
    const played = game({ last_played_at: "2026-07-25T00:00:00+00:00" });
    expect(pickFeatured([played])?.reason).toBe("Jump Back In");
  });

  it("does not claim a completed game is being continued", () => {
    const done = game({
      completion_state: "completed",
      last_played_at: "2026-07-25T00:00:00+00:00",
    });
    expect(pickFeatured([done])?.reason).toBe("Jump Back In");
  });
});

describe("pickFeatured fallbacks", () => {
  it("falls back to a favourite when nothing has been played", () => {
    const fav = game({ title: "Fav", is_favorite: 1 });
    const other = game({ title: "Other" });
    const featured = pickFeatured([other, fav]);
    expect(featured?.game.title).toBe("Fav");
    expect(featured?.reason).toBe("Favorite");
  });

  it("falls back to the most played when there are no favourites or play dates", () => {
    const little = game({ title: "Little", total_playtime_seconds: 10 });
    const lots = game({ title: "Lots", total_playtime_seconds: 9999 });
    const featured = pickFeatured([little, lots]);
    expect(featured?.game.title).toBe("Lots");
    expect(featured?.reason).toBe("Most Played");
  });

  it("falls back to the newest addition for an untouched library", () => {
    const old = game({ title: "Old", added_at: "2026-01-01T00:00:00+00:00" });
    const fresh = game({ title: "Fresh", added_at: "2026-07-01T00:00:00+00:00" });
    const featured = pickFeatured([old, fresh]);
    expect(featured?.game.title).toBe("Fresh");
    expect(featured?.reason).toBe("Recently Added");
  });

  it("returns null for an empty library rather than throwing", () => {
    expect(pickFeatured([])).toBeNull();
  });
});
