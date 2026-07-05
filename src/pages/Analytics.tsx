import { useEffect, useMemo, useState } from "react";
import { api } from "@/lib/ipc";
import type { HeatmapCell } from "@/types";
import { formatPlaytime } from "@/lib/format";
import { Icon } from "@/components/Icon";
import { StatCard } from "@/components/StatCard";

const CELL = 12;
const GAP = 3;
const LEFT_PAD = 30;
const TOP_PAD = 18;

const MONTHS = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

interface Week {
  days: (HeatmapCell | null)[];
  firstDate: Date;
}

export function Analytics() {
  const [cells, setCells] = useState<HeatmapCell[]>([]);

  useEffect(() => {
    api.heatmap(365).then(setCells);
  }, []);

  const weeks = useMemo(() => buildHeatmap(cells), [cells]);
  const max = Math.max(1, ...cells.map((c) => c.seconds));

  const summary = useMemo(() => {
    const active = cells.filter((c) => c.seconds > 0);
    const totalSeconds = active.reduce((sum, c) => sum + c.seconds, 0);
    const best = active.reduce(
      (acc, c) => (c.seconds > acc.seconds ? c : acc),
      { day: "", seconds: 0 }
    );
    return {
      totalSeconds,
      activeDays: active.length,
      streak: longestStreak(active.map((c) => c.day)),
      bestDay: best.seconds,
    };
  }, [cells]);

  return (
    <>
      <div className="page-head fade-up">
        <h2 className="page-title">Analytics</h2>
        <div className="page-sub">Your play activity over the last year.</div>
      </div>

      <div className="stat-grid">
        <StatCard
          icon="clock"
          label="Hours this year"
          value={formatPlaytime(summary.totalSeconds)}
          tone="cyan"
          index={0}
        />
        <StatCard
          icon="calendar"
          label="Active days"
          value={summary.activeDays.toString()}
          tone="violet"
          index={1}
        />
        <StatCard
          icon="flame"
          label="Longest streak"
          value={`${summary.streak} day${summary.streak === 1 ? "" : "s"}`}
          tone="gold"
          index={2}
        />
        <StatCard
          icon="zap"
          label="Best day"
          value={formatPlaytime(summary.bestDay)}
          tone="green"
          index={3}
        />
      </div>

      <div className="section-header">
        <h2>
          <Icon name="chart" size={15} />
          Activity heatmap
        </h2>
        <span className="sub">Active playtime per day · last 365 days</span>
      </div>

      <div className="panel heatmap-panel fade-up" style={{ animationDelay: "120ms" }}>
        <svg
          width={LEFT_PAD + weeks.length * (CELL + GAP)}
          height={TOP_PAD + 7 * (CELL + GAP)}
        >
          {/* month labels */}
          {weeks.map((w, x) => {
            const prev = weeks[x - 1];
            const next = weeks[x + 1];
            const month = w.firstDate.getMonth();
            if (prev && prev.firstDate.getMonth() === month) return null;
            // Skip a label in the very first column if the next column starts
            // a new month — the two labels would overlap.
            if (!prev && next && next.firstDate.getMonth() !== month) return null;
            return (
              <text
                key={`m-${x}`}
                x={LEFT_PAD + x * (CELL + GAP)}
                y={11}
                fontSize={10}
                fill="var(--text-tertiary)"
              >
                {MONTHS[month]}
              </text>
            );
          })}
          {/* weekday labels (grid starts on Sunday) */}
          {[
            { row: 1, label: "Mon" },
            { row: 3, label: "Wed" },
            { row: 5, label: "Fri" },
          ].map(({ row, label }) => (
            <text
              key={label}
              x={0}
              y={TOP_PAD + row * (CELL + GAP) + CELL - 2}
              fontSize={10}
              fill="var(--text-tertiary)"
            >
              {label}
            </text>
          ))}
          {/* day cells */}
          {weeks.map((w, x) =>
            w.days.map((c, y) => {
              const intensity = c ? c.seconds / max : 0;
              return (
                <rect
                  key={`${x}-${y}`}
                  x={LEFT_PAD + x * (CELL + GAP)}
                  y={TOP_PAD + y * (CELL + GAP)}
                  width={CELL}
                  height={CELL}
                  rx={3}
                  fill={
                    intensity === 0 ? "var(--bg-3)" : colorForIntensity(intensity)
                  }
                >
                  <title>
                    {c ? `${c.day}: ${formatPlaytime(c.seconds)}` : ""}
                  </title>
                </rect>
              );
            })
          )}
        </svg>

        <div className="heatmap-legend">
          Less
          <span className="swatch" style={{ background: "var(--bg-3)" }} />
          {[0.15, 0.35, 0.6, 0.8, 1].map((t) => (
            <span
              key={t}
              className="swatch"
              style={{ background: colorForIntensity(t) }}
            />
          ))}
          More
        </div>
      </div>
    </>
  );
}

function buildHeatmap(cells: HeatmapCell[]): Week[] {
  const byDay = new Map(cells.map((c) => [c.day, c]));
  const days: { date: Date; cell: HeatmapCell | null }[] = [];
  const end = new Date();
  end.setHours(0, 0, 0, 0);
  const start = new Date(end);
  start.setDate(start.getDate() - 364);
  // Pad start back to Sunday so the grid aligns.
  start.setDate(start.getDate() - start.getDay());
  for (let d = new Date(start); d <= end; d.setDate(d.getDate() + 1)) {
    const iso = d.toISOString().slice(0, 10);
    days.push({ date: new Date(d), cell: byDay.get(iso) ?? null });
  }
  const weeks: Week[] = [];
  for (let i = 0; i < days.length; i += 7) {
    const chunk = days.slice(i, i + 7);
    weeks.push({
      days: chunk.map((d) => d.cell),
      firstDate: chunk[0].date,
    });
  }
  return weeks;
}

function longestStreak(activeDaysIso: string[]): number {
  const sorted = [...activeDaysIso].sort();
  let best = 0;
  let cur = 0;
  let prev: string | null = null;
  for (const day of sorted) {
    cur = prev !== null && nextDay(prev) === day ? cur + 1 : 1;
    if (cur > best) best = cur;
    prev = day;
  }
  return best;
}

function nextDay(iso: string): string {
  const d = new Date(`${iso}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + 1);
  return d.toISOString().slice(0, 10);
}

function colorForIntensity(t: number): string {
  // 5-stop gradient: violet → cyan, ramped by sqrt for visibility.
  const k = Math.pow(t, 0.5);
  if (k < 0.2) return "rgba(124, 92, 255, 0.25)";
  if (k < 0.4) return "rgba(124, 92, 255, 0.45)";
  if (k < 0.65) return "rgba(124, 92, 255, 0.7)";
  if (k < 0.85) return "rgba(56, 189, 248, 0.85)";
  return "rgba(56, 189, 248, 1)";
}
