import { useEffect } from "react";
import { Routes, Route, Navigate, useLocation } from "react-router-dom";

import { Sidebar } from "@/components/Sidebar";
import { TopBar } from "@/components/TopBar";
import { ToastContainer } from "@/components/ToastContainer";
import { ErrorBoundary } from "@/components/ErrorBoundary";

import { Dashboard } from "@/pages/Dashboard";
import { Library } from "@/pages/Library";
import { GameDetails } from "@/pages/GameDetails";
import { Achievements } from "@/pages/Achievements";
import { SaveManager } from "@/pages/SaveManager";
import { Analytics } from "@/pages/Analytics";
import { Mods } from "@/pages/Mods";
import { Timeline } from "@/pages/Timeline";
import { Settings } from "@/pages/Settings";

import { useLibrary } from "@/stores/library";
import { onEvent } from "@/lib/ipc";
import { debounce, REFRESH_DEBOUNCE_MS } from "@/lib/debounce";
import { reportError } from "@/lib/toast";

export default function App() {
  const load = useLibrary((s) => s.load);
  const loc = useLocation();

  // Initial library load + subscribe to backend events to refresh.
  useEffect(() => {
    load().catch((e) => reportError(e, "load your library"));
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    // A background artwork fill emits one event per game, so without this each
    // one would cost a full `list_games` round trip.
    const refresh = debounce(() => {
      load().catch((e) => reportError(e, "refresh your library"));
    }, REFRESH_DEBOUNCE_MS);

    onEvent((ev) => {
      if (
        ev.type === "game_added" ||
        ev.type === "game_updated" ||
        ev.type === "session_ended" ||
        ev.type === "achievement_unlocked"
      ) {
        refresh();
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      refresh.cancel();
      unlisten?.();
    };
  }, [load]);

  return (
    <div className="app-shell">
      <Sidebar />
      <div className="main">
        <TopBar />
        {/* Keyed by path so the enter animation replays on navigation. */}
        <div className="content" key={loc.pathname}>
          {/* Scoped inside the shell so a page-level crash keeps navigation
              usable, and reset on route change so moving away recovers. */}
          <ErrorBoundary resetKey={loc.pathname}>
            <Routes>
              <Route path="/" element={<Navigate to="/dashboard" replace />} />
              <Route path="/dashboard" element={<Dashboard />} />
              <Route path="/library" element={<Library />} />
              <Route path="/library/:id" element={<GameDetails />} />
              <Route path="/library/:id/achievements" element={<Achievements />} />
              <Route path="/library/:id/saves" element={<SaveManager />} />
              <Route path="/library/:id/mods" element={<Mods />} />
              <Route path="/analytics" element={<Analytics />} />
              <Route path="/timeline" element={<Timeline />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="*" element={<Navigate to="/dashboard" replace />} />
            </Routes>
          </ErrorBoundary>
        </div>
      </div>
      <ToastContainer />
    </div>
  );
}
