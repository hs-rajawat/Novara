import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { api } from "@/lib/ipc";
import type { CompletionState, GameWithInstalls, Installation } from "@/types";
import { formatBytes, formatPlaytime, formatRelative } from "@/lib/format";
import { Icon } from "@/components/Icon";
import { StatCard } from "@/components/StatCard";
import { GameArtwork } from "@/components/GameArtwork";

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
  const [game, setGame] = useState<GameWithInstalls | null>(null);
  const [notes, setNotes] = useState("");
  const [savingNotes, setSavingNotes] = useState(false);
  const [launching, setLaunching] = useState(false);

  useEffect(() => {
    api.getGame(id).then((g) => {
      setGame(g);
      setNotes(g?.user_notes ?? "");
    });
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
    } finally {
      setLaunching(false);
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
          <div className="details-sub">
            {game.developer ?? "Unknown developer"} ·{" "}
            {game.release_year ?? "—"}
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
                : "No launchable installation found"
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
              <span className="chip">{formatBytes(i.install_size_bytes ?? 0)}</span>
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
                title="Choose a different executable for this installation"
              >
                <Icon name="folder" size={13} />
                Browse
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
  return installs.some((i) => i.executable !== null || i.source_app_id !== null);
}
