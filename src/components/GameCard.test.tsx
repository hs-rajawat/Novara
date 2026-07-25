// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

import { GameCard } from "./GameCard";
import type { Game } from "@/types";

// The library grid's card is the most-used interactive element in the app, and it
// nested a <button> inside an <a>. That is invalid HTML, and browsers disagree
// about the resulting pointer, keyboard and assistive-technology behaviour. The
// fix was structural — the navigation anchor is now a sibling overlay — so these
// tests assert the structure, not the styling.

vi.mock("@/lib/ipc", () => ({
  api: { launchGame: vi.fn(), setHidden: vi.fn() },
  onEvent: vi.fn(),
}));

vi.mock("@/stores/library", () => ({
  useLibrary: (selector: (s: unknown) => unknown) =>
    selector({ toggleFavorite: vi.fn(), load: vi.fn() }),
}));

afterEach(cleanup);

const game: Game = {
  id: "g1",
  title: "Hollow Knight",
  sort_title: "hollow knight",
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
  added_at: "2026-07-25T00:00:00+00:00",
  updated_at: "2026-07-25T00:00:00+00:00",
  primary_source_code: "steam",
  primary_source_label: "Steam",
  primary_install_status: "installed",
} as Game;

function renderCard(overrides: Partial<Game> = {}) {
  return render(
    <MemoryRouter>
      <GameCard game={{ ...game, ...overrides }} />
    </MemoryRouter>
  );
}

describe("GameCard markup", () => {
  it("does not nest a button inside the navigation link", () => {
    const { container } = renderCard();
    expect(container.querySelector("a button")).toBeNull();
    expect(container.querySelectorAll("button").length).toBeGreaterThan(0);
  });

  it("does not nest a link inside a button either", () => {
    const { container } = renderCard();
    expect(container.querySelector("button a")).toBeNull();
  });

  it("exposes one navigation link with an accessible name", () => {
    renderCard();
    const link = screen.getByRole("link", {
      name: "View details for Hollow Knight",
    });
    expect(link.getAttribute("href")).toBe("/library/g1");
  });

  it("keeps the link and the action buttons as siblings", () => {
    const { container } = renderCard();
    const link = container.querySelector("a.game-card-link")!;
    const card = container.querySelector("article.game-card")!;
    expect(link.parentElement).toBe(card);
    for (const button of Array.from(container.querySelectorAll("button"))) {
      expect(link.contains(button)).toBe(false);
    }
  });
});

describe("GameCard accessible names", () => {
  it("names the play control, rather than relying on a title attribute", () => {
    renderCard();
    expect(screen.getByRole("button", { name: "Play Hollow Knight" })).toBeTruthy();
  });

  it("names the favourite control and reports its pressed state", () => {
    renderCard();
    const fav = screen.getByRole("button", {
      name: "Add Hollow Knight to favorites",
    });
    expect(fav.getAttribute("aria-pressed")).toBe("false");
  });

  it("reflects an already-favourited game in aria-pressed", () => {
    renderCard({ is_favorite: 1 });
    const fav = screen.getByRole("button", {
      name: "Remove Hollow Knight from favorites",
    });
    expect(fav.getAttribute("aria-pressed")).toBe("true");
  });

  it("explains a missing game on the disabled play control", () => {
    renderCard({ primary_install_status: "missing" });
    const button = screen.getByRole("button", {
      name: /is missing — open details to relocate or remove it/,
    });
    expect(button).toHaveProperty("disabled", true);
  });

  it("offers a named restore control for a hidden game", () => {
    renderCard({ is_hidden: 1 });
    expect(
      screen.getByRole("button", { name: "Restore Hollow Knight to your library" })
    ).toBeTruthy();
  });

  it("hides the decorative chevron from assistive technology", () => {
    const { container } = renderCard();
    const decorative = container.querySelector(".qa-details");
    expect(decorative?.getAttribute("aria-hidden")).toBe("true");
  });
});
