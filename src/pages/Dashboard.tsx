import { useEffect, useState } from "react";
import { api, onEvent } from "@/lib/ipc";
import { toImgSrc } from "@/lib/image";
import { coverGradient } from "@/lib/color";
import type { DashboardStats, HeatmapCell } from "@/types";
import { formatPlaytime, formatRelative } from "@/lib/format";
import { Link } from "react-router-dom";
import { Icon } from "@/components/Icon";
import { StatCard } from "@/components/StatCard";
import { EmptyState } from "@/components/EmptyState";
import {
  ResponsiveContainer,
  BarChart,
  Bar,
  XAxis,
  Tooltip,
  CartesianGrid,
} from "recharts";

function greeting(): string {
  const h = new Date().getHours();
  if (h < 5) return "Up late";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

export function Dashboard() {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [heat, setHeat] = useState<HeatmapCell[]>([]);

  async function loadStats() {
    const [s, h] = await Promise.all([api.dashboardStats(), api.heatmap(90)]);
    setStats(s);
    setHeat(h);
  }

  useEffect(() => {
    loadStats().catch(console.error);
  }, []);

  // Refresh when gameplay or scan events arrive.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    onEvent((ev) => {
      if (
        ev.type === "scan_finished" ||
        ev.type === "session_ended" ||
        ev.type === "game_added" ||
        ev.type === "game_updated"
      ) {
        loadStats().catch(console.error);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  if (!stats) {
    return (
      <div className="stat-grid">
        {[0, 1, 2, 3].map((i) => (
          <div key={i} className="skeleton-card" style={{ height: 76 }}>
            <div className="skeleton-line shimmer" style={{ marginTop: 18 }} />
            <div className="skeleton-line shimmer short" />
          </div>
        ))}
      </div>
    );
  }

  const chartData = heat.map((c) => ({
    day: c.day.slice(5),
    hours: +(c.seconds / 3600).toFixed(2),
  }));

  return (
    <>
      <div className="page-head fade-up">
        <h2 className="page-title">
          {greeting()}<span className="grad">.</span>
        </h2>
        <div className="page-sub">
          Your local-first game library, all in one place.
        </div>
      </div>

      <div className="stat-grid">
        <StatCard
          icon="gamepad"
          label="Total games"
          value={stats.total_games.toString()}
          tone="violet"
          index={0}
        />
        <StatCard
          icon="trophy"
          label="Completed"
          value={stats.completed_games.toString()}
          tone="green"
          index={1}
        />
        <StatCard
          icon="clock"
          label="Hours played"
          value={formatPlaytime(stats.total_playtime_seconds)}
          tone="cyan"
          index={2}
        />
        <StatCard
          icon="star"
          label="Favorites"
          value={stats.favorite_count.toString()}
          tone="gold"
          index={3}
        />
      </div>

      <div className="section-header">
        <h2>
          <Icon name="chart" size={15} />
          Activity
        </h2>
        <span className="sub">Last 90 days</span>
      </div>
      <div
        className="panel fade-up"
        style={{ height: 230, marginBottom: 28, animationDelay: "120ms" }}
      >
        {chartData.length === 0 ? (
          <div className="empty" style={{ padding: "48px 20px" }}>
            <p>No sessions yet — sessions appear here once you play a game.</p>
          </div>
        ) : (
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={chartData} barCategoryGap="22%">
              <CartesianGrid
                stroke="var(--border-soft)"
                strokeDasharray="3 6"
                vertical={false}
              />
              <XAxis
                dataKey="day"
                stroke="var(--text-tertiary)"
                fontSize={11}
                tickLine={false}
                axisLine={false}
              />
              <Tooltip
                cursor={{ fill: "rgba(255, 255, 255, 0.04)" }}
                contentStyle={{
                  background: "var(--bg-glass)",
                  backdropFilter: "blur(12px)",
                  border: "1px solid var(--border-strong)",
                  borderRadius: 10,
                  fontSize: 12,
                  boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
                }}
                labelStyle={{ color: "var(--text-secondary)" }}
                itemStyle={{ color: "var(--accent-bright)" }}
              />
              <Bar
                dataKey="hours"
                fill="url(#g)"
                radius={[5, 5, 0, 0]}
                maxBarSize={26}
              />
              <defs>
                <linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--accent-2)" />
                  <stop offset="100%" stopColor="var(--accent)" />
                </linearGradient>
              </defs>
            </BarChart>
          </ResponsiveContainer>
        )}
      </div>

      <div className="section-header">
        <h2>
          <Icon name="history" size={15} />
          Recently played
        </h2>
        <Link to="/library" className="sub back-link" style={{ padding: "4px 10px" }}>
          View library
          <Icon name="chevron-right" size={13} />
        </Link>
      </div>

      {stats.recently_played.length === 0 ? (
        <EmptyState icon="gamepad" title="No games played yet">
          Add a scan path in Settings, run a scan, then press Play on any game
          to see it here.
        </EmptyState>
      ) : (
        <div className="list fade-up" style={{ marginBottom: 28, animationDelay: "180ms" }}>
          {stats.recently_played.map((g) => {
            const cover = toImgSrc(g.cover_path);
            return (
              <Link key={g.id} to={`/library/${g.id}`} className="list-row">
                <div
                  className="recent-thumb"
                  style={cover ? undefined : { background: coverGradient(g.title) }}
                >
                  {cover ? (
                    <img src={cover} alt={g.title} />
                  ) : (
                    <span>{g.title.slice(0, 2).toUpperCase()}</span>
                  )}
                </div>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontWeight: 600 }}>{g.title}</div>
                  <div className="small muted">
                    {formatRelative(g.last_played_at)} ·{" "}
                    {formatPlaytime(g.total_playtime_seconds)}
                  </div>
                </div>
                <Icon name="chevron-right" size={16} className="row-chevron" />
              </Link>
            );
          })}
        </div>
      )}

      {stats.top_genres.length > 0 && (
        <>
          <div className="section-header">
            <h2>
              <Icon name="sparkles" size={15} />
              Top genres
            </h2>
          </div>
          <div className="genre-chips" style={{ marginBottom: 28 }}>
            {stats.top_genres.map((g) => (
              <div key={g.name} className="genre-chip">
                <span className="genre-name">{g.name}</span>
                <span className="genre-count">{g.count}</span>
              </div>
            ))}
          </div>
        </>
      )}
    </>
  );
}
