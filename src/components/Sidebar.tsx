import { NavLink, useLocation } from "react-router-dom";
import { useLayoutEffect, useRef, useState } from "react";
import clsx from "clsx";
import { Icon, type IconName } from "@/components/Icon";

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
          Game<span className="brand-grad">Vault</span>
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
        v0.1.0 · local-first · no telemetry
      </div>
    </aside>
  );
}
