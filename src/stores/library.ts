// Library store — single source of truth for games. Refresh on any
// backend event that could affect the list.

import { create } from "zustand";
import { api } from "@/lib/ipc";
import { isLauncherManaged } from "@/lib/sources";
import type { Game } from "@/types";

export type SortMode =
  | "title_asc"
  | "title_desc"
  | "playtime"
  | "last_played"
  | "added";

const SORT_KEY = "library_sort";

function loadSort(): SortMode {
  return (localStorage.getItem(SORT_KEY) as SortMode) ?? "title_asc";
}

interface LibraryState {
  games: Game[];
  loading: boolean;
  error: string | null;
  query: string;
  filter: "all" | "favorites" | "playing" | "completed" | "backlog";
  sort: SortMode;
  /** Whether games removed from the library (`is_hidden`) are included. */
  includeHidden: boolean;
  load: () => Promise<void>;
  setQuery: (q: string) => void;
  setFilter: (f: LibraryState["filter"]) => void;
  setSort: (s: SortMode) => void;
  setIncludeHidden: (v: boolean) => void;
  toggleFavorite: (id: string) => Promise<void>;
}

export const useLibrary = create<LibraryState>((set, get) => ({
  games: [],
  loading: false,
  error: null,
  query: "",
  filter: "all",
  sort: loadSort(),
  includeHidden: false,
  load: async () => {
    set({ loading: true, error: null });
    try {
      const games = await api.listGames(get().includeHidden);
      set({ games, loading: false });
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },
  setQuery: (query) => set({ query }),
  setFilter: (filter) => set({ filter }),
  setSort: (sort) => {
    localStorage.setItem(SORT_KEY, sort);
    set({ sort });
  },
  setIncludeHidden: (includeHidden) => {
    set({ includeHidden });
    get().load();
  },
  toggleFavorite: async (id) => {
    const target = get().games.find((g) => g.id === id);
    if (!target) return;
    const next = !target.is_favorite;
    set({
      games: get().games.map((g) =>
        g.id === id ? { ...g, is_favorite: next ? 1 : 0 } : g
      ),
    });
    try {
      await api.setFavorite(id, next);
    } catch (err) {
      await get().load();
      throw err;
    }
  },
}));

export function selectVisibleGames(state: LibraryState): Game[] {
  const q = state.query.trim().toLowerCase();
  const filtered = state.games.filter((g) => {
    // Launcher-managed games (Steam, Epic, …) NOVARA can no longer
    // confirm as installed are hidden from the default Library view — their
    // history stays intact and reachable directly, they just don't clutter
    // the active grid. Manual/other-source Missing games stay visible (they
    // need the Missing badge / Locate Executable / Remove from Library flow).
    if (isLauncherManaged(g.primary_source_code) && g.primary_install_status === "missing") {
      return false;
    }
    if (q && !g.title.toLowerCase().includes(q)) return false;
    switch (state.filter) {
      case "favorites":
        return !!g.is_favorite;
      case "playing":
        return g.completion_state === "playing";
      case "completed":
        return g.completion_state === "completed";
      case "backlog":
        return g.completion_state === "backlog";
      default:
        return true;
    }
  });

  return filtered.slice().sort((a, b) => {
    switch (state.sort) {
      case "title_desc":
        return b.sort_title.localeCompare(a.sort_title);
      case "playtime":
        return b.total_playtime_seconds - a.total_playtime_seconds;
      case "last_played": {
        if (!a.last_played_at && !b.last_played_at) return 0;
        if (!a.last_played_at) return 1;
        if (!b.last_played_at) return -1;
        return b.last_played_at.localeCompare(a.last_played_at);
      }
      case "added":
        return b.added_at.localeCompare(a.added_at);
      default:
        // title_asc
        return a.sort_title.localeCompare(b.sort_title);
    }
  });
}
