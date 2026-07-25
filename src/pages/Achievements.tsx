import { useEffect, useState, type FormEvent } from "react";
import { useParams, Link } from "react-router-dom";
import clsx from "clsx";
import { api } from "@/lib/ipc";
import { reportError } from "@/lib/toast";
import type { Achievement } from "@/types";
import { Icon } from "@/components/Icon";
import { EmptyState } from "@/components/EmptyState";

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
    load().catch((e) => reportError(e, "load achievements"));
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
    } catch (err) {
      // Roll the optimistic update back, and say why — previously the row
      // silently snapped back with no explanation.
      reportError(err, "update this achievement");
      await load().catch(() => {});
    }
  }

  async function add(e: FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    try {
      await api.createAchievement({
        game_id: id,
        name,
        description: desc || undefined,
        category,
      });
      setName("");
      setDesc("");
      await load();
    } catch (err) {
      reportError(err, "add this achievement");
    }
  }

  const unlocked = items.filter((a) => a.is_unlocked).length;
  const total = items.length;
  const pct = total ? Math.round((unlocked / total) * 100) : 0;

  return (
    <>
      <div className="page-head fade-up">
        <Link to={`/library/${id}`} className="back-link">
          <Icon name="arrow-left" size={14} />
          Game
        </Link>
        <h2 className="page-title" style={{ marginTop: 8 }}>
          Achievements
        </h2>
        <div className="page-sub">
          {unlocked}/{total} unlocked · {pct}%
        </div>
      </div>

      {total > 0 && (
        <div className="progress fade-up" style={{ marginBottom: 24 }}>
          <div className="progress-fill" style={{ width: `${pct}%` }} />
        </div>
      )}

      <form
        onSubmit={add}
        className="panel fade-up"
        style={{ marginBottom: 22, display: "grid", gap: 10, animationDelay: "60ms" }}
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
            <Icon name="plus" size={14} />
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
        <EmptyState icon="trophy" title="No achievements yet">
          Create your own — or import a community template later.
        </EmptyState>
      ) : (
        <div className="list fade-up" style={{ animationDelay: "120ms" }}>
          {items.map((a) => (
            <div key={a.id} className="list-row">
              <button
                className={clsx("check", a.is_unlocked && "on")}
                onClick={() => toggle(a)}
                title="Toggle unlocked"
              >
                {a.is_unlocked ? <Icon name="check" size={13} /> : null}
              </button>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div
                  style={{
                    fontWeight: 600,
                    display: "flex",
                    alignItems: "center",
                    gap: 7,
                  }}
                >
                  {a.name}
                  {a.is_unlocked ? (
                    <Icon
                      name="trophy"
                      size={13}
                      style={{ color: "var(--warning)" }}
                    />
                  ) : null}
                </div>
                {a.description && (
                  <div className="muted small">{a.description}</div>
                )}
              </div>
              {a.category ? <span className="chip">{a.category}</span> : null}
              <span className="chip">{a.points} pts</span>
              <button
                className="btn btn-ghost icon-btn"
                style={{ width: 30, height: 30, flexBasis: 30 }}
                title="Delete achievement"
                onClick={async () => {
                  await api.deleteAchievement(a.id);
                  await load();
                }}
              >
                <Icon name="trash" size={14} />
              </button>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
