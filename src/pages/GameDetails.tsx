import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { api, onEvent } from "@/lib/ipc";
import type { CompletionState, GameWithInstalls, Installation, PlaySession } from "@/types";
import {
  formatBytes,
  formatPlaytime,
  formatRelative,
  formatSessionDay,
  formatSessionTime,
} from "@/lib/format";
import { Icon } from "@/components/Icon";
import { StatCard } from "@/components/StatCard";
import { GameArtwork } from "@/components/GameArtwork";
import { PlatformBadge } from "@/components/PlatformBadge";

const RECENT_SESSIONS_LIMIT = 8;

const STATES: CompletionState[] = [
  "unplayed",
  "playing",
  "backlog",
  "completed",
  "abandoned",
];

const IMG_FILTERS = [
  { name: "Images", extensions: ["jpg", "jpeg", "png", "gif", "webp"] },
];

export function GameDetails() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const [game, setGame] = useState<GameWithInstalls | null>(null);
  const [notes, setNotes] = useState("");
  const [savingNotes, setSavingNotes] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [sessions, setSessions] = useState<PlaySession[]>([]);

  useEffect(() => {
    api.getGame(id).then((g) => {
      setGame(g);
      setNotes(g?.user_notes ?? "");
    });
  }, [id]);

  useEffect(() => {
    api.listSessions(id, RECENT_SESSIONS_LIMIT).then(setSessions);
  }, [id]);

  // A session that ends, or the Library Integrity System changing this
  // game's status (periodic sweep, a launch-time recheck, Locate
  // Executable), should show up without a manual refresh.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    onEvent((ev) => {
      if (ev.type === "session_ended" && ev.game_id === id) {
        api.listSessions(id, RECENT_SESSIONS_LIMIT).then(setSessions);
      }
      if (ev.type === "game_updated" && ev.game_id === id) {
        api.getGame(id).then(setGame);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [id]);

  if (!game) {
    return (
      <div className="skeleton-card" style={{ height: 250 }}>
        <div className="shimmer" style={{ height: "100%" }} />
      </div>
    );
  }

  async function setState(s: CompletionState) {
    await api.setCompletion(id, game!.completion_pct, s);
    setGame(await api.getGame(id));
  }

  async function fav() {
    await api.setFavorite(id, !game!.is_favorite);
    setGame(await api.getGame(id));
  }

  async function saveNotes() {
    setSavingNotes(true);
    try {
      await api.updateNotes(id, notes || null);
    } finally {
      setSavingNotes(false);
    }
  }

  async function launch() {
    setLaunching(true);
    try {
      await api.launchGame(id);
    } catch {
      // Surfaced to the user via the backend's Notice toast.
    } finally {
      setLaunching(false);
    }
  }

  async function toggleHidden() {
    if (!game) return;
    const hidden = !game.is_hidden;
    if (
      hidden &&
      !confirm(
        `Remove "${game.title}" from your library? This keeps its playtime, sessions, and achievements — you can bring it back any time.`
      )
    ) {
      return;
    }
    await api.setHidden(id, hidden);
    if (hidden) {
      navigate("/library");
    } else {
      setGame(await api.getGame(id));
    }
  }

  async function browseExe(installationId: string) {
    const picked = await open({
      multiple: false,
      filters: [
        { name: "Executables", extensions: ["exe", "bat", "sh", "AppImage"] },
      ],
    });
    if (!picked || Array.isArray(picked)) return;
    await api.setInstallationExecutable(installationId, picked as string);
    setGame(await api.getGame(id));
  }

  async function pickCover() {
    const picked = await open({ multiple: false, filters: IMG_FILTERS });
    if (!picked || Array.isArray(picked)) return;
    const stored = await api.setCoverPath(id, picked as string);
    setGame((prev) => (prev ? { ...prev, cover_path: stored } : prev));
  }

  async function pickHero() {
    const picked = await open({ multiple: false, filters: IMG_FILTERS });
    if (!picked || Array.isArray(picked)) return;
    const stored = await api.setHeroPath(id, picked as string);
    setGame((prev) => (prev ? { ...prev, hero_path: stored } : prev));
  }

  return (
    <>
      {/* Hero banner with floating cover */}
      <div className="details-hero fade-up">
        <Link to="/library" className="details-back">
          <Icon name="arrow-left" size={14} />
          Library
        </Link>
        <GameArtwork
          src={game.hero_path}
          title={game.title}
          kind="hero"
          className="artwork-fill"
          alt={`${game.title} banner`}
          eager
        />
      </div>

      <div className="details-head">
        <div className="details-cover fade-up" style={{ animationDelay: "60ms" }}>
          <GameArtwork
            src={game.cover_path}
            title={game.title}
            kind="cover"
            alt={`${game.title} cover`}
            eager
          />
        </div>

        <div className="details-title-block fade-up" style={{ animationDelay: "100ms" }}>
          <h1 className="details-title">{game.title}</h1>
          <div className="details-meta-row">
            {game.developer && <span className="chip">{game.developer}</span>}
            {game.release_year && <span className="chip">{game.release_year}</span>}
            <PlatformBadge code={game.primary_source_code} label={game.primary_source_label} />
            {game.primary_install_status === "missing" && (
              <span className="chip chip-missing">
                <Icon name="alert-triangle" size={11} />
                Missing
              </span>
            )}
            <span className="chip cap">{game.completion_state}</span>
            <span className="chip">{formatPlaytime(game.total_playtime_seconds)}</span>
          </div>
        </div>

        <div className="details-actions fade-up" style={{ animationDelay: "140ms" }}>
          <button
            className="btn btn-primary btn-lg"
            onClick={launch}
            disabled={launching || !canLaunch(game.installations)}
            title={
              canLaunch(game.installations)
                ? "Launch game"
                : "No launchable installation found — locate its executable below"
            }
          >
            <Icon name="play" size={15} />
            {launching ? "Launching…" : "Play"}
          </button>
          <button
            className={clsx("btn icon-btn", game.is_favorite && "fav-on")}
            onClick={fav}
            title={game.is_favorite ? "Remove from favorites" : "Add to favorites"}
          >
            <Icon name="star" size={16} />
          </button>
          <Link to={`/library/${id}/achievements`} className="btn">
            <Icon name="trophy" size={15} />
            Achievements
          </Link>
          <Link to={`/library/${id}/saves`} className="btn">
            <Icon name="save" size={15} />
            Saves
          </Link>
          <Link to={`/library/${id}/mods`} className="btn">
            <Icon name="package" size={15} />
            Mods
          </Link>
          <button
            className="btn icon-btn"
            onClick={toggleHidden}
            title={game.is_hidden ? "Restore to library" : "Remove from library"}
          >
            <Icon name={game.is_hidden ? "rotate-ccw" : "trash"} size={16} />
          </button>
        </div>
      </div>

      <div className="stat-grid">
        <StatCard
          icon="clock"
          label="Playtime"
          value={formatPlaytime(game.total_playtime_seconds)}
          tone="cyan"
          index={0}
        />
        <StatCard
          icon="target"
          label="Completion"
          value={`${Math.round(game.completion_pct)}%`}
          tone="violet"
          index={1}
        />
        <StatCard
          icon="calendar"
          label="Last played"
          value={formatRelative(game.last_played_at)}
          tone="gold"
          index={2}
        />
        <StatCard
          icon="zap"
          label="Status"
          value={
            game.completion_state.charAt(0).toUpperCase() +
            game.completion_state.slice(1)
          }
          tone="green"
          index={3}
        />
      </div>

      <div className="section-header">
        <h2>
          <Icon name="history" size={15} />
          Recent sessions
        </h2>
        <Link to="/timeline" className="sub back-link">
          Full timeline
          <Icon name="chevron-right" size={13} />
        </Link>
      </div>
      {sessions.length === 0 ? (
        <div className="empty" style={{ padding: "36px 20px", marginBottom: 28 }}>
          <p>No sessions recorded yet — play this game to start tracking.</p>
        </div>
      ) : (
        <div className="list" style={{ marginBottom: 28 }}>
          {sessions.map((s) => (
            <div key={s.id} className="list-row">
              <div className="session-icon">
                <Icon name="zap" size={15} />
              </div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontWeight: 600 }}>{formatSessionDay(s.started_at)}</div>
                <div className="muted small">
                  {formatSessionTime(s.started_at)}
                  {s.idle_seconds > 0 ? ` · idle ${formatPlaytime(s.idle_seconds)}` : ""}
                </div>
              </div>
              <span className="chip">
                <Icon name="clock" size={11} />
                {formatPlaytime(s.duration_seconds)}
              </span>
            </div>
          ))}
        </div>
      )}

      <div className="section-header">
        <h2>
          <Icon name="target" size={15} />
          State
        </h2>
      </div>
      <div className="seg-tabs" style={{ marginBottom: 28 }}>
        {STATES.map((s) => (
          <button
            key={s}
            className={clsx("seg-tab cap", game.completion_state === s && "active")}
            onClick={() => setState(s)}
          >
            {s}
          </button>
        ))}
      </div>

      <div className="section-header">
        <h2>
          <Icon name="image" size={15} />
          Artwork
        </h2>
      </div>
      <div className="artwork-row" style={{ marginBottom: 28 }}>
        <div className="artwork-slot">
          <div className="artwork-preview artwork-preview-cover">
            <GameArtwork
              src={game.cover_path}
              title={game.title}
              kind="cover"
              className="artwork-fill"
              alt="Cover"
            />
          </div>
          <button className="btn btn-sm" onClick={pickCover}>
            <Icon name="image" size={13} />
            {game.cover_path ? "Change cover" : "Set cover"}
          </button>
        </div>
        <div className="artwork-slot">
          <div className="artwork-preview artwork-preview-hero">
            <GameArtwork
              src={game.hero_path}
              title={game.title}
              kind="hero"
              className="artwork-fill"
              alt="Hero"
            />
          </div>
          <button className="btn btn-sm" onClick={pickHero}>
            <Icon name="image" size={13} />
            {game.hero_path ? "Change hero" : "Set hero"}
          </button>
        </div>
      </div>

      <div className="section-header">
        <h2>
          <Icon name="hard-drive" size={15} />
          Installations
        </h2>
        <span className="sub">{game.installations.length} found</span>
      </div>
      <div className="list" style={{ marginBottom: 28 }}>
        {game.installations.map((i) => (
          <div
            key={i.id}
            className="list-row"
            style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}
          >
            <div className="row spread">
              <div className="mono" style={{ flex: 1 }}>
                {i.install_dir}
              </div>
              {i.status === "missing" ? (
                <span className="save-badge missing">
                  <Icon name="alert-triangle" size={11} />
                  Missing
                </span>
              ) : i.status === "deleted" ? (
                <span className="save-badge deleted">
                  <Icon name="alert-triangle" size={11} />
                  Deleted
                </span>
              ) : i.status === "offline" ? (
                <span className="save-badge offline">
                  <Icon name="alert-triangle" size={11} />
                  Drive offline
                </span>
              ) : (
                <span className="chip">{formatBytes(i.install_size_bytes ?? 0)}</span>
              )}
            </div>
            <div className="row spread">
              <div className="mono muted" style={{ flex: 1 }}>
                {i.executable ?? "no executable detected"}
                {i.executable && i.executable_override ? (
                  <span className="save-badge manual" style={{ marginLeft: 8 }}>
                    Manual
                  </span>
                ) : null}
              </div>
              <button
                className="btn btn-ghost btn-sm"
                onClick={() => browseExe(i.id)}
                title={
                  i.status === "missing"
                    ? "Point NOVARA at this game's executable to restore it"
                    : "Choose a different executable for this installation"
                }
              >
                <Icon name="folder" size={13} />
                {i.status === "missing" ? "Locate Executable" : "Browse"}
              </button>
            </div>
          </div>
        ))}
      </div>

      <div className="section-header">
        <h2>
          <Icon name="file-text" size={15} />
          Notes
        </h2>
        <button className="btn btn-sm" onClick={saveNotes} disabled={savingNotes}>
          <Icon name="check" size={13} />
          {savingNotes ? "Saving…" : "Save"}
        </button>
      </div>
      <textarea
        rows={6}
        style={{ width: "100%" }}
        value={notes}
        onChange={(e) => setNotes(e.target.value)}
        placeholder="Where am I in the story? Build I'm using? Side-quests left…"
      />
    </>
  );
}

function canLaunch(installs: Installation[]): boolean {
  return installs.some((i) => i.status === "installed");
}
