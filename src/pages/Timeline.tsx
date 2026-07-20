import { useEffect, useMemo, useState } from "react";
import { api } from "@/lib/ipc";
import type { Game, PlaySession } from "@/types";
import { formatPlaytime, formatSessionDay, formatSessionTime } from "@/lib/format";
import { Icon } from "@/components/Icon";
import { EmptyState } from "@/components/EmptyState";

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

  // Group sessions by calendar day, preserving backend order (newest first).
  const groups = useMemo(() => {
    const map = new Map<string, PlaySession[]>();
    for (const s of sessions) {
      const day = s.started_at.slice(0, 10);
      const bucket = map.get(day);
      if (bucket) bucket.push(s);
      else map.set(day, [s]);
    }
    return [...map.entries()];
  }, [sessions]);

  return (
    <>
      <div className="page-head fade-up">
        <h2 className="page-title">Session timeline</h2>
        <div className="page-sub">{sessions.length} sessions recorded</div>
      </div>

      {sessions.length === 0 ? (
        <EmptyState icon="history" title="No sessions yet">
          Sessions appear here once NOVARA detects a tracked game running.
        </EmptyState>
      ) : (
        groups.map(([day, daySessions], gi) => (
          <div
            key={day}
            className="timeline-group fade-up"
            style={{ animationDelay: `${Math.min(gi, 8) * 50}ms` }}
          >
            <div className="timeline-day">{formatSessionDay(day)}</div>
            <div className="list">
              {daySessions.map((s) => (
                <div key={s.id} className="list-row">
                  <div className="session-icon">
                    <Icon name="zap" size={15} />
                  </div>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: 600 }}>
                      {games.get(s.game_id)?.title ?? "Unknown game"}
                    </div>
                    <div className="muted small">
                      {formatSessionTime(s.started_at)}
                      {s.idle_seconds > 0
                        ? ` · idle ${formatPlaytime(s.idle_seconds)}`
                        : ""}
                    </div>
                  </div>
                  <span className="chip">
                    <Icon name="clock" size={11} />
                    {formatPlaytime(s.duration_seconds)}
                  </span>
                  {s.process_name ? (
                    <span className="mono muted small">{s.process_name}</span>
                  ) : null}
                </div>
              ))}
            </div>
          </div>
        ))
      )}
    </>
  );
}
