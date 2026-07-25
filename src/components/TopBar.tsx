import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router-dom";
import { useLibrary } from "@/stores/library";
import { api } from "@/lib/ipc";
import { reportError } from "@/lib/toast";
import { Icon } from "@/components/Icon";

const TITLES: Record<string, string> = {
  "/dashboard": "Dashboard",
  "/library": "Library",
  "/analytics": "Analytics",
  "/timeline": "Timeline",
  "/settings": "Settings",
};

function isTypingTarget(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    (el as HTMLElement).isContentEditable
  );
}

export function TopBar() {
  const loc = useLocation();
  const query = useLibrary((s) => s.query);
  const setQuery = useLibrary((s) => s.setQuery);
  const load = useLibrary((s) => s.load);
  const [scanning, setScanning] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);

  const title =
    TITLES[loc.pathname] ??
    (loc.pathname.startsWith("/library/") ? "Game Details" : "NOVARA");

  // "/" focuses search from anywhere; Escape clears + blurs it.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "/" && !isTypingTarget(document.activeElement)) {
        e.preventDefault();
        searchRef.current?.focus();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  async function scanNow() {
    setScanning(true);
    try {
      await api.scanNow();
      await load();
    } catch (e) {
      reportError(e, "scan for games");
    } finally {
      setScanning(false);
    }
  }

  return (
    <div className="topbar">
      <h1>{title}</h1>
      <div className="search">
        <Icon name="search" size={15} className="search-icon" />
        <input
          ref={searchRef}
          placeholder="Search library…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setQuery("");
              e.currentTarget.blur();
            }
          }}
        />
        <kbd>/</kbd>
      </div>
      <button
        className="btn btn-primary"
        onClick={scanNow}
        disabled={scanning}
        title="Scan all configured paths for installed games"
      >
        <Icon name="refresh" size={15} className={scanning ? "spin" : undefined} />
        {scanning ? "Scanning…" : "Scan now"}
      </button>
    </div>
  );
}
