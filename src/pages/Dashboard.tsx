import { useEffect, useState } from "react";
import { api } from "@/lib/ipc";
import type { DashboardStats, HeatmapCell } from "@/types";
import { formatPlaytime, formatRelative } from "@/lib/format";
import { Link } from "react-router-dom";
import {
  ResponsiveContainer,
  BarChart,
  Bar,
  XAxis,
  Tooltip,
  CartesianGrid,
} from "recharts";

export function Dashboard() {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [heat, setHeat] = useState<HeatmapCell[]>([]);

  useEffect(() => {
    api.dashboardStats().then(setStats).catch(console.error);
    api.heatmap(90).then(setHeat).catch(console.error);
  }, []);

  if (!stats) return <div className="empty">Loading…</div>;

  const chartData = heat.map((c) => ({
    day: c.day.slice(5),
    hours: +(c.seconds / 3600).toFixed(2),
  }));

  return (
    <>
      <div className="section-header">
        <div>
          <h2>Welcome back</h2>
          <div className="sub">
            Your local-first game library, all in one place.
          </div>
        </div>
      </div>

      <div className="stat-grid">
        <Stat label="Total games" value={stats.total_games.toString()} />
        <Stat label="Completed" value={stats.completed_games.toString()} />
        <Stat
          label="Hours played"
          value={formatPlaytime(stats.total_playtime_seconds)}
        />
        <Stat label="Favorites" value={stats.favorite_count.toString()} />
      </div>

      <div className="section-header">
        <h2>Activity (last 90 days)</h2>
      </div>
      <div
        style={{
          background: "var(--bg-1)",
          border: "1px solid var(--border-soft)",
          borderRadius: "var(--radius-md)",
          padding: 16,
          height: 220,
          marginBottom: 28,
        }}
      >
        {chartData.length === 0 ? (
          <div className="empty" style={{ padding: 40 }}>
            No play sessions yet — sessions appear here once you start playing.
          </div>
        ) : (
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={chartData}>
              <CartesianGrid stroke="var(--border-soft)" vertical={false} />
              <XAxis
                dataKey="day"
                stroke="var(--text-tertiary)"
                fontSize={11}
                tickLine={false}
              />
              <Tooltip
                contentStyle={{
                  background: "var(--bg-2)",
                  border: "1px solid var(--border-strong)",
                  borderRadius: 8,
                }}
              />
              <Bar dataKey="hours" fill="url(#g)" radius={[4, 4, 0, 0]} />
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
        <h2>Recently played</h2>
        <Link to="/library" className="sub">
          View library →
        </Link>
      </div>

      {stats.recently_played.length === 0 ? (
        <div className="empty">
          <h3>No games yet</h3>
          <div>Add a scan path in Settings, then press “Scan now”.</div>
        </div>
      ) : (
        <div className="list">
          {stats.recently_played.map((g) => (
            <Link
              key={g.id}
              to={`/library/${g.id}`}
              className="list-row"
              style={{ color: "inherit" }}
            >
              <div style={{ flex: 1 }}>
                <div style={{ fontWeight: 600 }}>{g.title}</div>
                <div className="small muted">
                  {formatRelative(g.last_played_at)} ·{" "}
                  {formatPlaytime(g.total_playtime_seconds)}
                </div>
              </div>
            </Link>
          ))}
        </div>
      )}
    </>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat-card">
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
    </div>
  );
}
