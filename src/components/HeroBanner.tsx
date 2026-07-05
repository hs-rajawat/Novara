import { useMemo, useState, type MouseEvent } from "react";
import { Link } from "react-router-dom";
import type { Game } from "@/types";
import { GameArtwork } from "@/components/GameArtwork";
import { PlatformBadge } from "@/components/PlatformBadge";
import { Icon } from "@/components/Icon";
import { api } from "@/lib/ipc";
import { toImgSrc } from "@/lib/image";
import { formatPlaytime, formatRelative } from "@/lib/format";

interface Props {
  games: Game[];
}

interface Featured {
  game: Game;
  reason: string;
}

/** Priority: favorite you're mid-playthrough on → most recently played →
 * any favorite → most played → newest addition. Always resolves to
 * something as long as `games` is non-empty. */
function pickFeatured(games: Game[]): Featured | null {
  if (games.length === 0) return null;

  const byLastPlayed = (a: Game, b: Game) =>
    (b.last_played_at ?? "").localeCompare(a.last_played_at ?? "");

  const favWithPlay = games
    .filter((g) => g.is_favorite && g.last_played_at)
    .sort(byLastPlayed)[0];
  if (favWithPlay) return { game: favWithPlay, reason: "Continue Playing" };

  const mostRecent = games.filter((g) => g.last_played_at).sort(byLastPlayed)[0];
  if (mostRecent) return { game: mostRecent, reason: "Jump Back In" };

  const anyFavorite = games.find((g) => g.is_favorite);
  if (anyFavorite) return { game: anyFavorite, reason: "Favorite" };

  const mostPlayed = games
    .filter((g) => g.total_playtime_seconds > 0)
    .sort((a, b) => b.total_playtime_seconds - a.total_playtime_seconds)[0];
  if (mostPlayed) return { game: mostPlayed, reason: "Most Played" };

  const newest = games.slice().sort((a, b) => b.added_at.localeCompare(a.added_at))[0];
  return { game: newest, reason: "Recently Added" };
}

export function HeroBanner({ games }: Props) {
  const featured = useMemo(() => pickFeatured(games), [games]);
  const [launching, setLaunching] = useState(false);

  if (!featured) return null;
  const { game, reason } = featured;
  const art = game.hero_path ?? game.cover_path;
  const ambientUrl = toImgSrc(art);

  async function handlePlay(e: MouseEvent) {
    e.preventDefault();
    if (launching) return;
    setLaunching(true);
    try {
      await api.launchGame(game.id);
    } finally {
      setLaunching(false);
    }
  }

  return (
    <div className="hero-banner fade-up">
      <div
        className="hero-ambient"
        style={ambientUrl ? { backgroundImage: `url("${ambientUrl}")` } : undefined}
      />
      <div className="hero-frame">
        <GameArtwork src={art} title={game.title} kind="hero" className="artwork-fill" eager />
        <div className="hero-content">
          <div className="hero-eyebrow">
            <Icon name="sparkles" size={13} />
            {reason}
          </div>
          <h1 className="hero-title">{game.title}</h1>
          <div className="hero-meta-row">
            <span className="chip cap">{game.completion_state}</span>
            <span className="chip">{formatPlaytime(game.total_playtime_seconds)}</span>
            {game.last_played_at && (
              <span className="chip">Played {formatRelative(game.last_played_at)}</span>
            )}
            <PlatformBadge code={game.primary_source_code} label={game.primary_source_label} />
          </div>
          <div className="hero-actions">
            <button
              type="button"
              className="btn btn-primary btn-lg"
              onClick={handlePlay}
              disabled={launching}
            >
              <Icon
                name={launching ? "refresh" : "play"}
                size={16}
                className={launching ? "spin" : undefined}
              />
              {launching ? "Launching…" : "Play"}
            </button>
            <Link to={`/library/${game.id}`} className="btn btn-lg btn-ghost-hero">
              <Icon name="info" size={15} />
              Details
            </Link>
          </div>
        </div>
      </div>
    </div>
  );
}

