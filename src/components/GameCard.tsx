import { useState, type MouseEvent } from "react";
import { Link } from "react-router-dom";
import clsx from "clsx";
import type { Game } from "@/types";
import { formatPlaytime, formatRelative } from "@/lib/format";
import { GameArtwork } from "@/components/GameArtwork";
import { PlatformBadge } from "@/components/PlatformBadge";
import { Icon } from "@/components/Icon";
import { api } from "@/lib/ipc";
import { reportError } from "@/lib/toast";
import { useLibrary } from "@/stores/library";

interface Props {
  game: Game;
  /** Position in the grid, used to stagger the entrance animation. */
  index?: number;
}

export function GameCard({ game, index = 0 }: Props) {
  const toggleFavorite = useLibrary((s) => s.toggleFavorite);
  const load = useLibrary((s) => s.load);
  const [launching, setLaunching] = useState(false);
  const missing = game.primary_install_status === "missing";

  async function handlePlay(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (launching || missing) return;
    setLaunching(true);
    try {
      await api.launchGame(game.id);
    } catch (err) {
      // The backend emits a Notice for conditions it anticipates (game
      // missing, drive offline), but not for unexpected failures — those
      // used to vanish here, leaving the Play button to silently do nothing.
      reportError(err, "launch this game");
    } finally {
      setLaunching(false);
    }
  }

  function handleFavorite(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    toggleFavorite(game.id);
  }

  async function handleRestore(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    try {
      await api.setHidden(game.id, false);
      await load();
    } catch (err) {
      reportError(err, "restore this game to your library");
    }
  }

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
        {game.completion_pct > 0 && (
          <span className="badge">
            <Icon name="check" size={10} />
            {Math.round(game.completion_pct)}%
          </span>
        )}
        <div className="cover-shade">
          {game.last_played_at
            ? `Played ${formatRelative(game.last_played_at)}`
            : "Never played"}
        </div>
        <div className="quick-actions">
          {game.is_hidden ? (
            <button
              type="button"
              className="qa-btn"
              onClick={handleRestore}
              title="Restore to library"
            >
              <Icon name="rotate-ccw" size={15} />
            </button>
          ) : (
            <button
              type="button"
              className="qa-btn qa-play"
              onClick={handlePlay}
              disabled={launching || missing}
              title={missing ? "Missing — open details to relocate or remove" : "Play"}
            >
              <Icon
                name={launching ? "refresh" : missing ? "alert-triangle" : "play"}
                size={16}
                className={launching ? "spin" : undefined}
              />
            </button>
          )}
          <button
            type="button"
            className={clsx("qa-btn", game.is_favorite && "is-on")}
            onClick={handleFavorite}
            title={game.is_favorite ? "Remove from favorites" : "Add to favorites"}
          >
            <Icon name="star" size={15} />
          </button>
          <span className="qa-btn qa-details" title="View details">
            <Icon name="chevron-right" size={16} />
          </span>
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
        <PlatformBadge code={game.primary_source_code} label={game.primary_source_label} />
      </div>
    </Link>
  );
}
