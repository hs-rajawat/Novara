import { NavLink } from "react-router-dom";
import clsx from "clsx";

const NAV = [
  { to: "/dashboard", label: "Dashboard", icon: "▦" },
  { to: "/library", label: "Library", icon: "▤" },
  { to: "/analytics", label: "Analytics", icon: "◔" },
  { to: "/timeline", label: "Timeline", icon: "—" },
];

const SETTINGS_NAV = [{ to: "/settings", label: "Settings", icon: "✦" }];

export function Sidebar() {
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <span className="logo">GV</span>
        <span>GameVault</span>
      </div>

      <div className="nav-section-label">Main</div>
      {NAV.map((n) => (
        <NavLink
          key={n.to}
          to={n.to}
          className={({ isActive }) => clsx("nav-item", isActive && "active")}
        >
          <span className="nav-icon" aria-hidden>
            {n.icon}
          </span>
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
          <span className="nav-icon" aria-hidden>
            {n.icon}
          </span>
          {n.label}
        </NavLink>
      ))}

      <div className="sidebar-footer">
        v0.1.0 · local-first · no telemetry
      </div>
    </aside>
  );
}
