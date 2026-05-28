import { Link } from "react-router-dom";
import type { Game } from "@/types";
import { formatPlaytime } from "@/lib/format";

interface Props {
  game: Game;
}

export function GameCard({ game }: Props) {
  const initials = game.title
    .split(/\s+/)
    .slice(0, 2)
    .map((s) => s[0])
    .join("")
    .toUpperCase();

  return (
    <Link to={`/library/${game.id}`} className="game-card">
      <div className="game-cover">
        {game.cover_path ? (
          <img src={convertCover(game.cover_path)} alt={game.title} />
        ) : (
          <span style={{ fontSize: 32, color: "var(--text-tertiary)" }}>
            {initials}
          </span>
        )}
        {game.is_favorite ? <span className="fav-pin">★</span> : null}
        {game.completion_state === "completed" && (
          <span className="badge">100%</span>
        )}
      </div>
      <div className="game-meta">
        <div className="game-title" title={game.title}>
          {game.title}
        </div>
        <div className="game-sub">
          {formatPlaytime(game.total_playtime_seconds)} · {game.completion_state}
        </div>
      </div>
    </Link>
  );
}

function convertCover(path: string): string {
  // Tauri assets get loaded via the `asset:` protocol; for local file
  // paths in user data we'd use `convertFileSrc`. Keep raw for now —
  // assets are added once a metadata provider downloads covers.
  return path.startsWith("http") || path.startsWith("data:")
    ? path
    : path;
}
