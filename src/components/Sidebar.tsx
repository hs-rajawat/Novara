import { NavLink, useLocation } from "react-router-dom";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import clsx from "clsx";
import { Icon, type IconName } from "@/components/Icon";
import { api } from "@/lib/ipc";

const NAV: { to: string; label: string; icon: IconName }[] = [
  { to: "/dashboard", label: "Dashboard", icon: "dashboard" },
  { to: "/library", label: "Library", icon: "library" },
  { to: "/analytics", label: "Analytics", icon: "chart" },
  { to: "/timeline", label: "Timeline", icon: "history" },
];

const SETTINGS_NAV: { to: string; label: string; icon: IconName }[] = [
  { to: "/settings", label: "Settings", icon: "settings" },
];

export function Sidebar() {
  const loc = useLocation();
  const railRef = useRef<HTMLElement>(null);
  const [indicatorTop, setIndicatorTop] = useState<number | null>(null);
  // Read from the backend rather than hardcoded. The footer said "v0.1.0" as a
  // literal, duplicating what `get_app_info` already reports from Cargo, so it
  // would have silently gone stale at the first version bump.
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    api
      .appInfo()
      .then((info) => setVersion(info.version))
      // A missing version is cosmetic: fall back to the product name rather than
      // interrupting the user over it.
      .catch(() => setVersion(null));
  }, []);

  // Measure the active .nav-item after each navigation rather than
  // re-deriving react-router's own active-matching rules here. Height isn't
  // measured — every .nav-item shares --nav-item-h, so the indicator's CSS
  // height already matches exactly.
  useLayoutEffect(() => {
    const active = railRef.current?.querySelector<HTMLElement>(".nav-item.active");
    setIndicatorTop(active ? active.offsetTop : null);
  }, [loc.pathname]);

  return (
    <aside className="sidebar" ref={railRef}>
      <div className="sidebar-brand">
        <span className="logo">
          <Icon name="gamepad" size={18} />
        </span>
        <span>
          NOVA<span className="brand-grad">RA</span>
        </span>
      </div>

      {indicatorTop !== null && (
        <div className="nav-indicator" style={{ top: indicatorTop }} />
      )}

      <div className="nav-section-label">Main</div>
      {NAV.map((n) => (
        <NavLink
          key={n.to}
          to={n.to}
          className={({ isActive }) => clsx("nav-item", isActive && "active")}
        >
          <Icon name={n.icon} size={16} />
          {n.label}
        </NavLink>
      ))}

      <div className="nav-section-label">System</div>
      {SETTINGS_NAV.map((n) => (
        <NavLink
          key={n.to}
          to={n.to}
          className={({ isActive }) => clsx("nav-item", isActive && "active")}
        >
          <Icon name={n.icon} size={16} />
          {n.label}
        </NavLink>
      ))}

      <div className="sidebar-footer">
        <span className="dot" />
        {version ? `v${version}` : "NOVARA"} · local-first · no telemetry
      </div>
    </aside>
  );
}
