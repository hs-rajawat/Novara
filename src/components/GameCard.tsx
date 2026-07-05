import { Link } from "react-router-dom";
import type { Game } from "@/types";
import { formatPlaytime, formatRelative } from "@/lib/format";
import { GameArtwork } from "@/components/GameArtwork";
import { Icon } from "@/components/Icon";

interface Props {
  game: Game;
  /** Position in the grid, used to stagger the entrance animation. */
  index?: number;
}

export function GameCard({ game, index = 0 }: Props) {
  return (
    <Link
      to={`/library/${game.id}`}
      className="game-card fade-up"
      style={{ animationDelay: `${Math.min(index, 16) * 35}ms` }}
    >
      <div className="game-cover">
        <GameArtwork src={game.cover_path} title={game.title} kind="cover" />
        {game.is_favorite ? (
          <span className="fav-pin">
            <Icon name="star" size={13} />
          </span>
        ) : null}
        {game.completion_state === "completed" && (
          <span className="badge">
            <Icon name="check" size={10} />
            100%
          </span>
        )}
        <div className="cover-shade">
          {game.last_played_at
            ? `Played ${formatRelative(game.last_played_at)}`
            : "Never played"}
        </div>
      </div>
      <div className="game-meta">
        <div className="game-title" title={game.title}>
          {game.title}
        </div>
        <div className="game-sub">
          <span className={`state-dot ${game.completion_state}`} />
          {formatPlaytime(game.total_playtime_seconds)} ·{" "}
          <span className="cap">{game.completion_state}</span>
        </div>
      </div>
    </Link>
  );
}
