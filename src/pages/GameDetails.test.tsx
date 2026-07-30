// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { GameDetails } from "./GameDetails";
import type { Achievement, GameWithInstalls, PlaySession, SaveProfile } from "@/types";

// GAME_DETAILS_REDESIGN.md makes the hero the whole first viewport, so these
// tests pin the composition rules that are easy to regress: identity lives in the
// hero, the logo replaces the visible title (without leaving the accessibility
// tree), statistics are metadata rather than dashboard cards, and the panels'
// deferred read stays deferred.

const listSaveProfiles = vi.fn(async (): Promise<SaveProfile[]> => []);
const getGame = vi.fn(async (): Promise<GameWithInstalls> => currentGame);
const listAchievements = vi.fn(async (): Promise<Achievement[]> => currentAchievements);

vi.mock("@/lib/ipc", () => ({
  api: {
    getGame: (...args: unknown[]) =>
      (getGame as unknown as (...a: unknown[]) => Promise<GameWithInstalls>)(...args),
    listSessions: vi.fn(async (): Promise<PlaySession[]> => []),
    listAchievements: (...args: unknown[]) =>
      (listAchievements as unknown as (...a: unknown[]) => Promise<Achievement[]>)(
        ...args
      ),
    listSaveProfiles: (...args: unknown[]) =>
      (listSaveProfiles as unknown as (...a: unknown[]) => Promise<SaveProfile[]>)(
        ...args
      ),
    setFavorite: vi.fn(),
    setCompletion: vi.fn(),
    updateNotes: vi.fn(),
    launchGame: vi.fn(),
    setHidden: vi.fn(),
    setInstallationExecutable: vi.fn(),
    refreshMetadata: vi.fn(),
  },
  onEvent: vi.fn(async () => () => {}),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const achievements: Achievement[] = [
  {
    id: "a1",
    game_id: "g1",
    template_id: null,
    name: "First Blood",
    description: null,
    category: null,
    icon_path: null,
    points: 10,
    is_secret: 0,
    is_unlocked: 1,
    unlocked_at: "2024-01-01T10:00:00Z",
    sort_order: 0,
  },
  {
    id: "a2",
    game_id: "g1",
    template_id: null,
    name: "True Ending",
    description: null,
    category: null,
    icon_path: null,
    points: 50,
    is_secret: 0,
    is_unlocked: 0,
    unlocked_at: null,
    sort_order: 1,
  },
];

const baseGame: GameWithInstalls = {
  id: "g1",
  title: "Hollow Knight",
  sort_title: "hollow knight",
  description: "Explore twisting caverns.",
  release_year: 2017,
  developer: "Team Cherry",
  publisher: "Cherry Publishing",
  cover_path: null,
  hero_path: null,
  icon_path: null,
  logo_path: null,
  metadata_json: JSON.stringify({
    "367520": {
      success: true,
      data: {
        genres: [{ description: "Metroidvania" }],
        // Steam's real casing, so the normalisation to a title-cased label is
        // genuinely exercised.
        categories: [{ description: "Full controller support" }],
        release_date: { date: "24 Feb, 2017" },
        platforms: { windows: true, mac: true, linux: false },
        // Eight, to exercise the About truncation limit of six.
        supported_languages:
          "English, French, German, Spanish, Japanese, Korean, Russian, Portuguese",
      },
    },
  }),
  metadata_source: "steam_cdn",
  is_favorite: 0,
  is_hidden: 0,
  completion_pct: 42,
  completion_state: "playing",
  user_rating: null,
  user_notes: null,
  total_playtime_seconds: 7200,
  last_played_at: null,
  added_at: "2024-01-01T00:00:00Z",
  updated_at: "2024-01-01T00:00:00Z",
  primary_source_code: "steam",
  primary_source_label: "Steam",
  primary_install_status: "installed",
  installations: [
    {
      id: "i1",
      game_id: "g1",
      source_id: 1,
      install_dir: "C:/Games/HollowKnight",
      executable: "hollow_knight.exe",
      launch_args: null,
      source_app_id: "367520",
      install_size_bytes: 1024,
      is_primary: 1,
      detected_at: "2024-01-01T00:00:00Z",
      executable_override: 0,
      status: "installed",
      last_verified_at: null,
    },
  ],
};

let currentGame: GameWithInstalls = baseGame;
let currentAchievements: Achievement[] = achievements;

afterEach(() => {
  cleanup();
  currentGame = baseGame;
  currentAchievements = achievements;
});

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/library/g1"]}>
      <Routes>
        <Route path="/library/:id" element={<GameDetails />} />
      </Routes>
    </MemoryRouter>
  );
}

const hero = (container: HTMLElement) =>
  within(container.querySelector(".gd-hero") as HTMLElement);

const achievementsCard = (container: HTMLElement) =>
  Array.from(container.querySelectorAll(".gd-panel .gd-card")).find(
    (c) => c.querySelector(".gd-card-head h2")?.textContent === "Achievements"
  ) as HTMLElement;

describe("GameDetails hero", () => {
  it("carries the whole identity: title, badges, description, metadata, actions", async () => {
    const { container } = renderPage();
    expect(await screen.findByRole("heading", { name: "Hollow Knight" })).toBeTruthy();
    const h = hero(container);

    expect(h.getByRole("heading", { name: "Hollow Knight" })).toBeTruthy();
    expect(h.getByText("Explore twisting caverns.")).toBeTruthy();

    const badges = within(container.querySelector(".gd-hero-badges") as HTMLElement);
    expect(badges.getByText("Steam")).toBeTruthy();
    expect(badges.getByText("Metroidvania")).toBeTruthy();
    expect(badges.getByText("42%")).toBeTruthy();
    // Year and completion state are stated below (metadata row, Library status
    // card), so they must not also be badges.
    expect(badges.queryByText("2017")).toBeNull();
    expect(badges.queryByText("playing")).toBeNull();
    expect(badges.queryByText(/complete/)).toBeNull();

    const meta = within(container.querySelector(".gd-hero-meta") as HTMLElement);
    expect(meta.getByText("Team Cherry")).toBeTruthy();
    expect(meta.getByText("Cherry Publishing")).toBeTruthy();
    expect(meta.getByText("24 Feb, 2017")).toBeTruthy();
    expect(meta.getByText("2.0h")).toBeTruthy();

    expect(h.getByRole("button", { name: /Play/ })).toBeTruthy();
    expect(h.getByRole("button", { name: "Favorite" })).toBeTruthy();
    expect(h.getByRole("link", { name: /Achievements/ })).toBeTruthy();
    expect(h.getByRole("link", { name: /Saves/ })).toBeTruthy();
    expect(h.getByRole("link", { name: /Mods/ })).toBeTruthy();
    expect(h.getByRole("button", { name: "Refresh metadata" })).toBeTruthy();
    expect(h.getByRole("button", { name: "Remove from library" })).toBeTruthy();
  });

  it("groups the action row as Play | game actions | utilities", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const row = container.querySelector(".gd-hero-actions") as HTMLElement;

    // Three groups, so the 24px-between / 8px-within rhythm comes from one rule
    // rather than from hand-placed spacers.
    const groups = Array.from(row.children);
    expect(groups.map((g) => g.className)).toEqual([
      "gd-action-group",
      "gd-action-group",
      "gd-action-group",
    ]);
    expect(groups.map((g) => g.children.length)).toEqual([1, 4, 2]);
    // No leftover spacer element doing the job the gaps now do.
    expect(row.querySelector(".gd-actions-gap")).toBeNull();

    // Play sits alone; prominence comes from colour, not bulk.
    const play = groups[0].firstElementChild as HTMLElement;
    expect(play.className).toContain("gd-btn-play");
    expect(play.className).toContain("gd-btn");

    // Every control shares the base class, so height, radius, border weight and
    // alignment all come from one rule.
    expect(row.querySelectorAll(".gd-btn")).toHaveLength(7);

    // The utility pair is square and identical.
    Array.from(groups[2].children).forEach((u) =>
      expect(u.className).toContain("gd-btn-icon")
    );

    // One icon size across the whole row.
    row.querySelectorAll("svg").forEach((svg) => {
      expect(svg.getAttribute("width")).toBe("16");
      expect(svg.getAttribute("height")).toBe("16");
    });
  });

  it("shows the text title when the game has no logo", async () => {
    const { container } = renderPage();
    const title = await screen.findByRole("heading", { name: "Hollow Knight" });
    expect(title.className).not.toContain("gd-visually-hidden");
    expect(container.querySelector(".gd-hero-logo")).toBeNull();
  });

  it("prefers the logo, keeping the heading for assistive technology", async () => {
    // An http URL because `toImgSrc` passes those through; a local path would go
    // through Tauri's asset protocol, which does not exist under jsdom.
    currentGame = { ...baseGame, logo_path: "https://cdn.test/logo.png" };
    const { container } = renderPage();
    const title = await screen.findByRole("heading", { name: "Hollow Knight" });
    // Hidden visually, still in the accessibility tree and the outline.
    expect(title.className).toContain("gd-visually-hidden");
    expect(container.querySelector(".gd-hero-logo")).not.toBeNull();
  });

  it("falls back to the text title when the logo fails to load", async () => {
    currentGame = { ...baseGame, logo_path: "https://cdn.test/broken.png" };
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const logo = container.querySelector(".gd-hero-logo") as HTMLImageElement;
    fireEvent.error(logo);
    expect(container.querySelector(".gd-hero-logo")).toBeNull();
    expect(
      screen.getByRole("heading", { name: "Hollow Knight" }).className
    ).not.toContain("gd-visually-hidden");
  });

  it("presents statistics as metadata, never as dashboard cards", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    expect(container.querySelector(".stat-card")).toBeNull();
    expect(container.querySelector(".stat-grid")).toBeNull();
    expect(container.querySelectorAll(".gd-meta-item").length).toBeGreaterThan(3);
  });

  it("renders hero artwork as width-fitted, not stretched to the box", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const art = container.querySelector(".gd-hero-art") as HTMLElement;
    expect(art).not.toBeNull();
    // `artwork-fill` would force the image to the hero's height and crop the
    // sides; the hero deliberately takes its art height from the source ratio.
    expect(art.className).not.toContain("artwork-fill");
    expect(art.className).toContain("artwork-hero");
  });

  it("puts the action row beneath the cover, not inside the text column", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const inner = container.querySelector(".gd-hero-inner") as HTMLElement;
    // Two stacked parts: poster+text, then the action row spanning both.
    expect(Array.from(inner.children).map((c) => c.className)).toEqual([
      "gd-hero-top",
      "gd-hero-actions",
    ]);
    // Nested in the text column it would start indented past the poster.
    expect(container.querySelector(".gd-hero-id .gd-hero-actions")).toBeNull();
  });

  it("leads the badge row with a source badge carrying its launcher mark", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const badges = container.querySelector(".gd-hero-badges") as HTMLElement;
    const source = badges.firstElementChild as HTMLElement;
    // First badge, and it is the source: identity before attributes.
    expect(source.className).toContain("platform-badge");
    expect(source.className).toContain("tone-steam");
    // The glyph replaces the tone dot rather than sitting next to it.
    expect(source.className).toContain("has-icon");
    expect(source.querySelector("svg")).not.toBeNull();
    expect(source.textContent).toBe("Steam");
  });

  it("falls back to a generic source mark for an unrecognised store", async () => {
    currentGame = {
      ...baseGame,
      primary_source_code: "someshop",
      primary_source_label: "Some Shop",
    };
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const source = container.querySelector(".gd-hero-badges")
      ?.firstElementChild as HTMLElement;
    expect(source.className).toContain("tone-default");
    expect(source.querySelector("svg")).not.toBeNull();
  });
});

describe("GameDetails panels", () => {
  it("opens on Overview with About facts parsed from stored metadata", async () => {
    renderPage();
    const overview = await screen.findByRole("tab", { name: "Overview" });
    expect(overview.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText("Full Controller Support")).toBeTruthy();
    // Installations were their own page section before; still reachable.
    expect(screen.getByText("C:/Games/HollowKnight")).toBeTruthy();
    // Developer and release date belong to the hero now, not About.
    expect(screen.getAllByText("Team Cherry")).toHaveLength(1);
  });

  it("has no library status control anywhere on the page", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    // Game Details is about the game. Where the completion state belongs globally
    // is a separate decision, so it is absent here rather than relocated.
    expect(container.querySelectorAll(".gd-state")).toHaveLength(0);
    expect(container.querySelector(".gd-statusbar")).toBeNull();
    for (const state of ["unplayed", "backlog", "completed", "abandoned"]) {
      expect(screen.queryByRole("button", { name: state })).toBeNull();
    }
  });

  it("orders the Overview cards by purpose", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    // Achievements | About
    // Notes        | Installations
    expect(
      Array.from(container.querySelectorAll(".gd-panel .gd-card-head h2")).map(
        (h) => h.textContent
      )
    ).toEqual(["Achievements", "Notes", "About", "Installations"]);
    // One progress bar on the page, not two telling overlapping stories.
    expect(container.querySelectorAll(".gd-bar")).toHaveLength(1);
  });

  it("lays the Achievements card out as percentage, bar, count, strip", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const card = achievementsCard(container);

    expect(card.querySelector(".gd-metric-value")?.textContent).toBe("50%");
    expect(card.querySelector(".gd-metric-caption")?.textContent).toBe("Complete");
    expect(card.querySelector(".gd-ach-count")?.textContent).toBe("1 / 2 unlocked");
    // A real outlined button, not a hyperlink.
    const viewAll = within(card).getByRole("button", {
      name: /View All/,
    });
    expect(viewAll.className).toContain("gd-btn-outline");

    // Count sits below the bar, strip below the count.
    expect(Array.from(card.children).map((c) => c.className)).toEqual([
      "gd-card-head",
      "gd-metric",
      "gd-bar",
      "gd-ach-count",
      "gd-ach-strip",
    ]);

    const tiles = card.querySelectorAll(".gd-ach-tile");
    expect(tiles).toHaveLength(2);
    expect(tiles[0].className).toContain("is-unlocked");
    expect((tiles[0] as HTMLElement).title).toBe("First Blood");
    // Locked entries are shown as placeholders rather than omitted.
    expect(tiles[1].className).not.toContain("is-unlocked");
    expect((tiles[1] as HTMLElement).title).toBe("True Ending");
  });

  it("looks finished before any achievements exist", async () => {
    currentAchievements = [];
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const card = achievementsCard(container);

    // No empty-state message: the same layout, with honest zeroes.
    expect(card.querySelector(".gd-empty")).toBeNull();
    expect(card.querySelector(".gd-metric-value")?.textContent).toBe("0%");
    expect(card.querySelector(".gd-ach-count")?.textContent).toBe("0 / 0 unlocked");
    expect(card.querySelector(".gd-bar")).not.toBeNull();

    // A full shelf of locked tiles, styled exactly like real locked ones — no
    // separate washed-out placeholder variant.
    const tiles = card.querySelectorAll(".gd-ach-tile");
    expect(tiles).toHaveLength(8);
    tiles.forEach((t) => expect(t.className).toBe("gd-ach-tile"));

    expect(
      within(card).getByRole("button", { name: /View All/ })
    ).toHaveProperty("disabled", true);
  });

  it("tiers the badge row by colour: neutral launcher, accent genre, amber progress", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const badges = container.querySelector(".gd-hero-badges") as HTMLElement;
    const [source, genre, progress] = Array.from(badges.children) as HTMLElement[];

    expect(source.className).toContain("platform-badge");
    expect(source.className).not.toContain("is-genre");
    expect(genre.className).toContain("is-genre");
    expect(genre.textContent).toBe("Metroidvania");
    expect(progress.className).toContain("is-progress");
    expect(progress.textContent).toBe("42%");
  });

  it("shows About as scannable chips, with the release date back in place", async () => {
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const about = Array.from(container.querySelectorAll(".gd-panel .gd-card")).find(
      (c) => c.querySelector(".gd-card-head h2")?.textContent === "About"
    ) as HTMLElement;

    // Taken out of the badge row deliberately; this is where it belongs.
    expect(within(about).getByText("Release date")).toBeTruthy();
    expect(within(about).getByText("24 Feb, 2017")).toBeTruthy();

    // Platform reads as OS marks, not prose.
    const platform = Array.from(about.querySelectorAll(".gd-fact")).find(
      (f) => f.querySelector("dt")?.textContent === "Platform"
    ) as HTMLElement;
    const chips = Array.from(platform.querySelectorAll(".gd-tag"));
    expect(chips.map((c) => c.textContent)).toEqual(["Windows", "macOS"]);
    chips.forEach((c) => expect(c.querySelector("svg")).not.toBeNull());

    // Controller is a mark plus a normalised label, not the provider's string.
    expect(within(about).getByText("Full Controller Support")).toBeTruthy();
  });

  it("curates Features rather than mirroring every store category", async () => {
    currentGame = {
      ...baseGame,
      metadata_json: JSON.stringify({
        "1": {
          success: true,
          data: {
            categories: [
              { description: "Single-player" },
              { description: "Steam Achievements" },
              { description: "Remote Play on Phone" },
              { description: "Family Sharing" },
              { description: "Stats" },
            ],
          },
        },
      }),
    };
    const { container } = renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    const features = Array.from(container.querySelectorAll(".gd-fact")).find(
      (f) => f.querySelector("dt")?.textContent === "Features"
    ) as HTMLElement;

    // Plumbing categories are dropped; only what a player cares about survives.
    expect(
      Array.from(features.querySelectorAll(".gd-tag")).map((c) => c.textContent)
    ).toEqual(["Single Player", "Achievements"]);
  });

  it("keeps About to what a player looks for, without the provider name", async () => {
    renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    expect(screen.getByText("Genre")).toBeTruthy();
    expect(screen.getByText("Controller")).toBeTruthy();
    expect(screen.getByText("Full Controller Support")).toBeTruthy();
    // Implementation detail: never shown to the user.
    expect(screen.queryByText(/metadata source/i)).toBeNull();
    expect(screen.queryByText("steam_cdn")).toBeNull();
  });

  it("truncates long About values and discloses the rest on request", async () => {
    renderPage();
    await screen.findByRole("heading", { name: "Hollow Knight" });
    // Eight languages, limit three — the longest value set on the card.
    expect(screen.getByText("English")).toBeTruthy();
    expect(screen.queryByText("Russian")).toBeNull();

    const more = screen.getByRole("button", { name: "+5 more" });
    fireEvent.click(more);
    expect(screen.getByText("Russian")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Show less" })).toBeTruthy();
  });

  it("switches panels when another tab is selected", async () => {
    renderPage();
    const tab = await screen.findByRole("tab", { name: "Activity" });
    fireEvent.click(tab);
    expect(tab.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText(/No sessions recorded yet/)).toBeTruthy();
    expect(screen.queryByText("Full Controller Support")).toBeNull();
  });

  it("reads save profiles only once the Saves tab is opened", async () => {
    renderPage();
    const tab = await screen.findByRole("tab", { name: "Saves" });
    expect(listSaveProfiles).not.toHaveBeenCalled();
    fireEvent.click(tab);
    await waitFor(() => expect(listSaveProfiles).toHaveBeenCalledTimes(1));
  });
});
