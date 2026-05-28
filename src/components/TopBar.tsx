import { useLocation } from "react-router-dom";
import { useLibrary } from "@/stores/library";
import { api } from "@/lib/ipc";
import { useState } from "react";

const TITLES: Record<string, string> = {
  "/dashboard": "Dashboard",
  "/library": "Library",
  "/analytics": "Analytics",
  "/timeline": "Timeline",
  "/settings": "Settings",
};

export function TopBar() {
  const loc = useLocation();
  const query = useLibrary((s) => s.query);
  const setQuery = useLibrary((s) => s.setQuery);
  const load = useLibrary((s) => s.load);
  const [scanning, setScanning] = useState(false);

  const title =
    TITLES[loc.pathname] ??
    (loc.pathname.startsWith("/library/") ? "Game Details" : "GameVault");

  async function scanNow() {
    setScanning(true);
    try {
      await api.scanNow();
      await load();
    } catch (e) {
      console.error(e);
    } finally {
      setScanning(false);
    }
  }

  return (
    <div className="topbar">
      <h1>{title}</h1>
      <input
        className="search"
        placeholder="Search library…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <button
        className="btn btn-primary"
        onClick={scanNow}
        disabled={scanning}
        title="Scan all configured paths for installed games"
      >
        {scanning ? "Scanning…" : "Scan now"}
      </button>
    </div>
  );
}
