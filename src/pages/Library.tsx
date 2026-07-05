import { useMemo } from "react";
import clsx from "clsx";
import { useLibrary, selectVisibleGames, type SortMode } from "@/stores/library";
import { GameCard } from "@/components/GameCard";
import { EmptyState } from "@/components/EmptyState";

type Filter = "all" | "favorites" | "playing" | "backlog" | "completed";

const FILTERS: { key: Filter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "favorites", label: "Favorites" },
  { key: "playing", label: "Playing" },
  { key: "backlog", label: "Backlog" },
  { key: "completed", label: "Completed" },
];

const SORTS: { key: SortMode; label: string }[] = [
  { key: "title_asc", label: "Title A → Z" },
  { key: "title_desc", label: "Title Z → A" },
  { key: "playtime", label: "Most played" },
  { key: "last_played", label: "Last played" },
  { key: "added", label: "Recently added" },
];

export function Library() {
  const games = useLibrary(selectVisibleGames);
  const allGames = useLibrary((s) => s.games);
  const loading = useLibrary((s) => s.loading);
  const filter = useLibrary((s) => s.filter);
  const sort = useLibrary((s) => s.sort);
  const setFilter = useLibrary((s) => s.setFilter);
  const setSort = useLibrary((s) => s.setSort);

  const counts = useMemo<Record<Filter, number>>(
    () => ({
      all: allGames.length,
      favorites: allGames.filter((g) => g.is_favorite).length,
      playing: allGames.filter((g) => g.completion_state === "playing").length,
      backlog: allGames.filter((g) => g.completion_state === "backlog").length,
      completed: allGames.filter((g) => g.completion_state === "completed").length,
    }),
    [allGames]
  );

  return (
    <>
      <div className="toolbar">
        <div className="seg-tabs">
          {FILTERS.map((f) => (
            <button
              key={f.key}
              className={clsx("seg-tab", filter === f.key && "active")}
              onClick={() => setFilter(f.key)}
            >
              {f.label}
              <span className="seg-count">{counts[f.key]}</span>
            </button>
          ))}
        </div>

        <label className="sort-control">
          Sort by
          <select
            value={sort}
            onChange={(e) => setSort(e.target.value as SortMode)}
          >
            {SORTS.map((s) => (
              <option key={s.key} value={s.key}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
      </div>

      {loading ? (
        <div className="game-grid">
          {Array.from({ length: 12 }, (_, i) => (
            <div key={i} className="skeleton-card">
              <div className="skeleton-cover shimmer" />
              <div className="skeleton-line shimmer" />
              <div className="skeleton-line shimmer short" />
            </div>
          ))}
        </div>
      ) : games.length === 0 ? (
        <EmptyState icon="search" title="No games match this filter">
          Try the "All" tab, adjust your search, or run a scan from the top
          bar to discover installed games.
        </EmptyState>
      ) : (
        <div className="game-grid">
          {games.map((g, i) => (
            <GameCard key={g.id} game={g} index={i} />
          ))}
        </div>
      )}
    </>
  );
}
