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
          Track installed mods, toggle them on or off, and control load order.
        </div>
      </div>
      {/* User-facing copy. This previously read "The data model is ready (mods
          table). Wire a per-game mods folder watcher to populate it." — a note to
          a developer shown to the person using the app. */}
      <EmptyState icon="package" title="Mod tracking isn't available yet">
        NOVARA will list the mods installed for this game here, so you can enable
        or disable them and set their load order. This feature is still in
        development.
      </EmptyState>
    </>
  );
}
