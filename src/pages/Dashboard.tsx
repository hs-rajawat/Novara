import { useEffect, useMemo, useState } from "react";
import { api, onEvent } from "@/lib/ipc";
import type { DashboardStats, HeatmapCell } from "@/types";
import { formatPlaytime } from "@/lib/format";
import { Icon } from "@/components/Icon";
import { StatCard } from "@/components/StatCard";
import { HeroBanner } from "@/components/HeroBanner";
import { Carousel } from "@/components/Carousel";
import { GameCard } from "@/components/GameCard";
import { EmptyLibrary } from "@/components/EmptyLibrary";
import { useLibrary } from "@/stores/library";
import {
  ResponsiveContainer,
  BarChart,
  Bar,
  XAxis,
  Tooltip,
  CartesianGrid,
} from "recharts";

const SHELF_SIZE = 15;

export function Dashboard() {
  const games = useLibrary((s) => s.games);
  const libLoading = useLibrary((s) => s.loading);
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

  const continuePlaying = useMemo(
    () =>
      games
        .filter((g) => g.completion_state === "playing")
        .sort((a, b) => (b.last_played_at ?? "").localeCompare(a.last_played_at ?? ""))
        .slice(0, SHELF_SIZE),
    [games]
  );

  const recentlyAdded = useMemo(
    () => games.slice().sort((a, b) => b.added_at.localeCompare(a.added_at)).slice(0, SHELF_SIZE),
    [games]
  );

  const mostPlayed = useMemo(
    () =>
      games
        .filter((g) => g.total_playtime_seconds > 0)
        .sort((a, b) => b.total_playtime_seconds - a.total_playtime_seconds)
        .slice(0, SHELF_SIZE),
    [games]
  );

  if (libLoading && games.length === 0) {
    return (
      <div className="skeleton-card fade-up" style={{ height: 420, borderRadius: "var(--radius-xl)" }}>
        <div className="shimmer" style={{ height: "100%" }} />
      </div>
    );
  }

  if (!libLoading && games.length === 0) {
    return <EmptyLibrary />;
  }

  const chartData = heat.map((c) => ({
    day: c.day.slice(5),
    hours: +(c.seconds / 3600).toFixed(2),
  }));

  return (
    <>
      <HeroBanner games={games} />

      {stats && (
        <div className="stat-strip fade-up" style={{ animationDelay: "60ms" }}>
          <div className="stat-pill">
            <Icon name="gamepad" size={14} />
            <strong>{stats.total_games}</strong> games
          </div>
          <div className="stat-pill">
            <Icon name="clock" size={14} />
            <strong>{formatPlaytime(stats.total_playtime_seconds)}</strong> played
          </div>
          <div className="stat-pill">
            <Icon name="star" size={14} />
            <strong>{stats.favorite_count}</strong> favorites
          </div>
          <div className="stat-pill">
            <Icon name="trophy" size={14} />
            <strong>{stats.completed_games}</strong> completed
          </div>
        </div>
      )}

      {continuePlaying.length > 0 && (
        <Carousel title="Continue Playing" icon="play" viewAllHref="/library">
          {continuePlaying.map((g, i) => (
            <GameCard key={g.id} game={g} index={i} />
          ))}
        </Carousel>
      )}

      {recentlyAdded.length > 0 && (
        <Carousel title="Recently Added" icon="plus" viewAllHref="/library">
          {recentlyAdded.map((g, i) => (
            <GameCard key={g.id} game={g} index={i} />
          ))}
        </Carousel>
      )}

      {mostPlayed.length > 0 && (
        <Carousel title="Most Played" icon="flame" viewAllHref="/library">
          {mostPlayed.map((g, i) => (
            <GameCard key={g.id} game={g} index={i} />
          ))}
        </Carousel>
      )}

      <div className="section-header">
        <h2>
          <Icon name="chart" size={15} />
          Insights
        </h2>
        <span className="sub">Last 90 days</span>
      </div>
      <div className="panel fade-up" style={{ height: 230, marginBottom: 28 }}>
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

      {stats && (
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
      )}

      {stats && stats.top_genres.length > 0 && (
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
