import { NavLink } from "react-router-dom";
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
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <span className="logo">
          <Icon name="gamepad" size={18} />
        </span>
        <span>
          Game<span className="brand-grad">Vault</span>
        </span>
      </div>

      <div className="nav-section-label">Main</div>
      {NAV.map((n) => (
        <NavLink
          key={n.to}
          to={n.to}
          className={({ isActive }) => clsx("nav-item", isActive && "active")}
        >
          <Icon name={n.icon} size={17} />
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
          <Icon name={n.icon} size={17} />
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
