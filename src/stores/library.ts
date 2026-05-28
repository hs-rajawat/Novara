// Library store — single source of truth for games. Refresh on any
// backend event that could affect the list.

import { create } from "zustand";
import { api } from "@/lib/ipc";
import type { Game } from "@/types";

interface LibraryState {
  games: Game[];
  loading: boolean;
  error: string | null;
  query: string;
  filter: "all" | "favorites" | "playing" | "completed" | "backlog";
  load: () => Promise<void>;
  setQuery: (q: string) => void;
  setFilter: (f: LibraryState["filter"]) => void;
  toggleFavorite: (id: string) => Promise<void>;
}

export const useLibrary = create<LibraryState>((set, get) => ({
  games: [],
  loading: false,
  error: null,
  query: "",
  filter: "all",
  load: async () => {
    set({ loading: true, error: null });
    try {
      const games = await api.listGames(false);
      set({ games, loading: false });
    } catch (err) {
      set({ error: String(err), loading: false });
    }
  },
  setQuery: (query) => set({ query }),
  setFilter: (filter) => set({ filter }),
  toggleFavorite: async (id) => {
    const target = get().games.find((g) => g.id === id);
    if (!target) return;
    const next = target.is_favorite ? false : true;
    // Optimistic — revert on failure.
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
  return state.games.filter((g) => {
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
}
