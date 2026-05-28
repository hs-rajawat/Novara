import { useEffect, useState, type FormEvent } from "react";
import { useParams, Link } from "react-router-dom";
import clsx from "clsx";
import { api } from "@/lib/ipc";
import type { Achievement } from "@/types";

export function Achievements() {
  const { id = "" } = useParams();
  const [items, setItems] = useState<Achievement[]>([]);
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [category, setCategory] = useState("custom");

  async function load() {
    setItems(await api.listAchievements(id));
  }
  useEffect(() => {
    load();
  }, [id]);

  async function toggle(a: Achievement) {
    // optimistic
    setItems((cur) =>
      cur.map((x) =>
        x.id === a.id ? { ...x, is_unlocked: x.is_unlocked ? 0 : 1 } : x
      )
    );
    try {
      await api.toggleAchievement(a.id);
    } catch {
      await load();
    }
  }

  async function add(e: FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    await api.createAchievement({
      game_id: id,
      name,
      description: desc || undefined,
      category,
    });
    setName("");
    setDesc("");
    await load();
  }

  const unlocked = items.filter((a) => a.is_unlocked).length;
  const total = items.length;
  const pct = total ? Math.round((unlocked / total) * 100) : 0;

  return (
    <>
      <div className="row spread" style={{ marginBottom: 16 }}>
        <div>
          <Link to={`/library/${id}`} className="muted small">
            ← Game
          </Link>
          <h2 style={{ margin: "6px 0 2px" }}>Achievements</h2>
          <div className="muted small">
            {unlocked}/{total} unlocked · {pct}%
          </div>
        </div>
      </div>

      <form
        onSubmit={add}
        className="list"
        style={{ padding: 14, marginBottom: 20, display: "grid", gap: 8 }}
      >
        <div className="row" style={{ gap: 8 }}>
          <input
            placeholder="Achievement name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            style={{ flex: 1 }}
          />
          <select value={category} onChange={(e) => setCategory(e.target.value)}>
            <option value="main">Main</option>
            <option value="side">Side</option>
            <option value="collectible">Collectible</option>
            <option value="ending">Ending</option>
            <option value="custom">Custom</option>
          </select>
          <button className="btn btn-primary" type="submit">
            Add
          </button>
        </div>
        <input
          placeholder="Description (optional)"
          value={desc}
          onChange={(e) => setDesc(e.target.value)}
        />
      </form>

      {items.length === 0 ? (
        <div className="empty">
          <h3>No achievements yet</h3>
          <div>Create your own — or import a community template later.</div>
        </div>
      ) : (
        <div className="list">
          {items.map((a) => (
            <div key={a.id} className="list-row">
              <button
                className={clsx("check", a.is_unlocked && "on")}
                onClick={() => toggle(a)}
                title="Toggle unlocked"
              >
                {a.is_unlocked ? "✓" : ""}
              </button>
              <div style={{ flex: 1 }}>
                <div style={{ fontWeight: 600 }}>{a.name}</div>
                {a.description && (
                  <div className="muted small">{a.description}</div>
                )}
              </div>
              <div className="muted small">{a.category ?? ""}</div>
              <div className="muted small">{a.points} pts</div>
              <button
                className="btn btn-ghost small"
                onClick={async () => {
                  await api.deleteAchievement(a.id);
                  await load();
                }}
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
