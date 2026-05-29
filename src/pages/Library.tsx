import clsx from "clsx";
import { useLibrary, selectVisibleGames, type SortMode } from "@/stores/library";
import { GameCard } from "@/components/GameCard";

type Filter = "all" | "favorites" | "playing" | "backlog" | "completed";

const FILTERS: { key: Filter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "favorites", label: "Favorites" },
  { key: "playing", label: "Playing" },
  { key: "backlog", label: "Backlog" },
  { key: "completed", label: "Completed" },
];

const SORTS: { key: SortMode; label: string }[] = [
  { key: "title_asc", label: "A → Z" },
  { key: "title_desc", label: "Z → A" },
  { key: "playtime", label: "Most played" },
  { key: "last_played", label: "Last played" },
  { key: "added", label: "Recently added" },
];

export function Library() {
  const games = useLibrary(selectVisibleGames);
  const loading = useLibrary((s) => s.loading);
  const filter = useLibrary((s) => s.filter);
  const sort = useLibrary((s) => s.sort);
  const setFilter = useLibrary((s) => s.setFilter);
  const setSort = useLibrary((s) => s.setSort);

  return (
    <>
      <div className="tabs">
        {FILTERS.map((f) => (
          <button
            key={f.key}
            className={clsx("tab", filter === f.key && "active")}
            onClick={() => setFilter(f.key)}
          >
            {f.label}
          </button>
        ))}
      </div>

      <div className="sort-bar">
        <span className="sort-label">Sort:</span>
        {SORTS.map((s) => (
          <button
            key={s.key}
            className={clsx("btn btn-ghost sort-btn", sort === s.key && "active")}
            onClick={() => setSort(s.key)}
          >
            {s.label}
          </button>
        ))}
      </div>

      {loading ? (
        <div className="empty">Loading library…</div>
      ) : games.length === 0 ? (
        <div className="empty">
          <h3>No games match this filter</h3>
          <div>Try "All", or scan again from the top bar.</div>
        </div>
      ) : (
        <div className="game-grid">
          {games.map((g) => (
            <GameCard key={g.id} game={g} />
          ))}
        </div>
      )}
    </>
  );
}
