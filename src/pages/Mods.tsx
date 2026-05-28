import { Link, useParams } from "react-router-dom";

export function Mods() {
  const { id = "" } = useParams();
  return (
    <>
      <div style={{ marginBottom: 16 }}>
        <Link to={`/library/${id}`} className="muted small">
          ← Game
        </Link>
        <h2 style={{ margin: "6px 0 2px" }}>Mods</h2>
        <div className="muted small">
          Filesystem-indexed mods with enable/disable + load order.
        </div>
      </div>
      <div className="empty">
        <h3>Mod tracking — coming next</h3>
        <div>
          The data model is ready (mods table). Wire a per-game mods folder
          watcher to populate it.
        </div>
      </div>
    </>
  );
}
