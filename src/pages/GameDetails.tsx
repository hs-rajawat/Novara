import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "@/lib/ipc";
import type { CompletionState, GameWithInstalls } from "@/types";
import { formatBytes, formatPlaytime, formatRelative } from "@/lib/format";

const STATES: CompletionState[] = [
  "unplayed",
  "playing",
  "backlog",
  "completed",
  "abandoned",
];

export function GameDetails() {
  const { id = "" } = useParams();
  const [game, setGame] = useState<GameWithInstalls | null>(null);
  const [notes, setNotes] = useState("");
  const [savingNotes, setSavingNotes] = useState(false);

  useEffect(() => {
    api.getGame(id).then((g) => {
      setGame(g);
      setNotes(g?.user_notes ?? "");
    });
  }, [id]);

  if (!game) return <div className="empty">Loading…</div>;

  async function setState(s: CompletionState) {
    await api.setCompletion(id, game!.completion_pct, s);
    const refreshed = await api.getGame(id);
    setGame(refreshed);
  }

  async function fav() {
    await api.setFavorite(id, !game!.is_favorite);
    const refreshed = await api.getGame(id);
    setGame(refreshed);
  }

  async function saveNotes() {
    setSavingNotes(true);
    try {
      await api.updateNotes(id, notes || null);
    } finally {
      setSavingNotes(false);
    }
  }

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <Link to="/library" className="muted small">
            ← Library
          </Link>
          <h2 style={{ margin: "6px 0 2px" }}>{game.title}</h2>
          <div className="muted small">
            {game.developer ?? "Unknown developer"} ·{" "}
            {game.release_year ?? "—"}
          </div>
        </div>
        <div className="row">
          <button className="btn" onClick={fav}>
            {game.is_favorite ? "★ Favorited" : "☆ Favorite"}
          </button>
          <Link to={`/library/${id}/achievements`} className="btn">
            Achievements
          </Link>
          <Link to={`/library/${id}/saves`} className="btn">
            Saves
          </Link>
          <Link to={`/library/${id}/mods`} className="btn">
            Mods
          </Link>
        </div>
      </div>

      <div className="stat-grid">
        <Stat
          label="Playtime"
          value={formatPlaytime(game.total_playtime_seconds)}
        />
        <Stat label="Completion" value={`${Math.round(game.completion_pct)}%`} />
        <Stat
          label="Last played"
          value={formatRelative(game.last_played_at)}
        />
        <Stat label="Status" value={game.completion_state} />
      </div>

      <div className="section-header">
        <h2>State</h2>
      </div>
      <div className="row wrap" style={{ marginBottom: 24 }}>
        {STATES.map((s) => (
          <button
            key={s}
            className={`btn ${
              game.completion_state === s ? "btn-primary" : ""
            }`}
            onClick={() => setState(s)}
          >
            {s}
          </button>
        ))}
      </div>

      <div className="section-header">
        <h2>Installations</h2>
        <span className="sub">{game.installations.length} found</span>
      </div>
      <div className="list" style={{ marginBottom: 24 }}>
        {game.installations.map((i) => (
          <div key={i.id} className="list-row">
            <div style={{ flex: 1, fontFamily: "var(--font-mono)", fontSize: 12 }}>
              {i.install_dir}
            </div>
            <div className="muted small">{formatBytes(i.install_size_bytes ?? 0)}</div>
          </div>
        ))}
      </div>

      <div className="section-header">
        <h2>Notes</h2>
        <button
          className="btn"
          onClick={saveNotes}
          disabled={savingNotes}
        >
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

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat-card">
      <div className="stat-label">{label}</div>
      <div className="stat-value" style={{ fontSize: 18 }}>
        {value}
      </div>
    </div>
  );
}
