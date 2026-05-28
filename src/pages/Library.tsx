import clsx from "clsx";
import { useLibrary, selectVisibleGames } from "@/stores/library";
import { GameCard } from "@/components/GameCard";

type Filter = "all" | "favorites" | "playing" | "backlog" | "completed";

const FILTERS: { key: Filter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "favorites", label: "Favorites" },
  { key: "playing", label: "Playing" },
  { key: "backlog", label: "Backlog" },
  { key: "completed", label: "Completed" },
];

export function Library() {
  const games = useLibrary(selectVisibleGames);
  const loading = useLibrary((s) => s.loading);
  const filter = useLibrary((s) => s.filter);
  const setFilter = useLibrary((s) => s.setFilter);

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

      {loading ? (
        <div className="empty">Loading library…</div>
      ) : games.length === 0 ? (
        <div className="empty">
          <h3>No games match this filter</h3>
          <div>Try “All”, or scan again from the top bar.</div>
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
