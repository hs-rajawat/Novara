import { useEffect } from "react";
import { Routes, Route, Navigate } from "react-router-dom";

import { Sidebar } from "@/components/Sidebar";
import { TopBar } from "@/components/TopBar";

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

export default function App() {
  const load = useLibrary((s) => s.load);

  // Initial library load + subscribe to backend events to refresh.
  useEffect(() => {
    load();
    let unlisten: (() => void) | null = null;
    onEvent((ev) => {
      if (
        ev.type === "game_added" ||
        ev.type === "game_updated" ||
        ev.type === "session_ended" ||
        ev.type === "achievement_unlocked"
      ) {
        load();
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [load]);

  return (
    <div className="app-shell">
      <Sidebar />
      <div className="main">
        <TopBar />
        <div className="content">
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
        </div>
      </div>
    </div>
  );
}
