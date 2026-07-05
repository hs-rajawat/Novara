import { Link, useParams } from "react-router-dom";
import { Icon } from "@/components/Icon";
import { EmptyState } from "@/components/EmptyState";

export function Mods() {
  const { id = "" } = useParams();
  return (
    <>
      <div className="page-head fade-up">
        <Link to={`/library/${id}`} className="back-link">
          <Icon name="arrow-left" size={14} />
          Game
        </Link>
        <h2 className="page-title" style={{ marginTop: 8 }}>
          Mods
        </h2>
        <div className="page-sub">
          Filesystem-indexed mods with enable/disable + load order.
        </div>
      </div>
      <EmptyState icon="package" title="Mod tracking — coming next">
        The data model is ready (mods table). Wire a per-game mods folder
        watcher to populate it.
      </EmptyState>
    </>
  );
}
