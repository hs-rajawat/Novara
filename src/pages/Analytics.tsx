import { useEffect, useMemo, useState } from "react";
import { api } from "@/lib/ipc";
import type { HeatmapCell } from "@/types";
import { formatPlaytime } from "@/lib/format";

const CELL = 12;
const GAP = 3;

export function Analytics() {
  const [cells, setCells] = useState<HeatmapCell[]>([]);

  useEffect(() => {
    api.heatmap(365).then(setCells);
  }, []);

  const grid = useMemo(() => buildHeatmap(cells), [cells]);
  const max = Math.max(1, ...cells.map((c) => c.seconds));

  return (
    <>
      <div className="section-header">
        <h2>Completion heatmap (last 365 days)</h2>
        <span className="sub">Active playtime per day</span>
      </div>

      <div
        style={{
          background: "var(--bg-1)",
          border: "1px solid var(--border-soft)",
          borderRadius: "var(--radius-md)",
          padding: 20,
          overflowX: "auto",
        }}
      >
        <svg
          width={(grid.length) * (CELL + GAP)}
          height={7 * (CELL + GAP)}
        >
          {grid.map((week, x) =>
            week.map((c, y) => {
              const intensity = c ? c.seconds / max : 0;
              return (
                <rect
                  key={`${x}-${y}`}
                  x={x * (CELL + GAP)}
                  y={y * (CELL + GAP)}
                  width={CELL}
                  height={CELL}
                  rx={2}
                  fill={intensity === 0 ? "var(--bg-3)" : colorForIntensity(intensity)}
                >
                  <title>
                    {c ? `${c.day}: ${formatPlaytime(c.seconds)}` : ""}
                  </title>
                </rect>
              );
            })
          )}
        </svg>
      </div>
    </>
  );
}

function buildHeatmap(cells: HeatmapCell[]): (HeatmapCell | null)[][] {
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
  const weeks: (HeatmapCell | null)[][] = [];
  for (let i = 0; i < days.length; i += 7) {
    weeks.push(days.slice(i, i + 7).map((d) => d.cell));
  }
  return weeks;
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
