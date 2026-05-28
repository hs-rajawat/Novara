import { useEffect, useState } from "react";
import { api } from "@/lib/ipc";
import type { Game, PlaySession } from "@/types";
import { formatPlaytime, formatRelative } from "@/lib/format";

export function Timeline() {
  const [sessions, setSessions] = useState<PlaySession[]>([]);
  const [games, setGames] = useState<Map<string, Game>>(new Map());

  useEffect(() => {
    (async () => {
      const [ss, gs] = await Promise.all([
        api.listSessions(undefined, 200),
        api.listGames(true),
      ]);
      setSessions(ss);
      setGames(new Map(gs.map((g) => [g.id, g])));
    })();
  }, []);

  return (
    <>
      <div className="section-header">
        <h2>Session timeline</h2>
        <span className="sub">{sessions.length} sessions</span>
      </div>

      {sessions.length === 0 ? (
        <div className="empty">
          <h3>No sessions yet</h3>
          <div>
            Sessions appear here once GameVault detects a tracked game running.
          </div>
        </div>
      ) : (
        <div className="list">
          {sessions.map((s) => (
            <div key={s.id} className="list-row">
              <div style={{ flex: 1 }}>
                <div style={{ fontWeight: 600 }}>
                  {games.get(s.game_id)?.title ?? "Unknown game"}
                </div>
                <div className="muted small">
                  Started {formatRelative(s.started_at)} ·{" "}
                  {formatPlaytime(s.duration_seconds)} (idle{" "}
                  {formatPlaytime(s.idle_seconds)})
                </div>
              </div>
              <div className="muted small">{s.process_name ?? ""}</div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
