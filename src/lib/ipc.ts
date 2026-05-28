// Thin typed wrapper around Tauri's invoke. Centralizes command names
// so renaming a backend command is a one-file change.

import { invoke } from "@tauri-apps/api/core";
import type {
  Achievement,
  AppEvent,
  DashboardStats,
  Game,
  GameWithInstalls,
  HeatmapCell,
  PlaySession,
  SaveBackup,
  SaveProfile,
  ScanReport,
} from "@/types";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const api = {
  // system
  appInfo: () => invoke<{ version: string; data_dir: string }>("get_app_info"),
  getSettings: () => invoke<Record<string, unknown>>("get_settings"),
  setSetting: (key: string, value: unknown) =>
    invoke<void>("set_setting", { key, value }),

  // games
  listGames: (includeHidden = false) =>
    invoke<Game[]>("list_games", { includeHidden }),
  getGame: (id: string) => invoke<GameWithInstalls | null>("get_game", { id }),
  setFavorite: (id: string, favorite: boolean) =>
    invoke<void>("set_favorite", { id, favorite }),
  setCompletion: (id: string, pct: number, completionState: string) =>
    invoke<void>("set_completion", { id, pct, completionState }),
  updateNotes: (id: string, notes: string | null) =>
    invoke<void>("update_notes", { id, notes }),
  mergeDuplicates: (fromId: string, toId: string) =>
    invoke<void>("merge_duplicates", { fromId, toId }),

  // scan
  scanNow: () => invoke<ScanReport[]>("scan_paths_now"),
  listScanPaths: () => invoke<string[]>("list_scan_paths"),
  addScanPath: (path: string) => invoke<void>("add_scan_path", { path }),
  removeScanPath: (path: string) => invoke<void>("remove_scan_path", { path }),

  // achievements
  listAchievements: (gameId: string) =>
    invoke<Achievement[]>("list_achievements", { gameId }),
  createAchievement: (input: {
    game_id: string;
    name: string;
    description?: string;
    category?: string;
    points?: number;
    is_secret?: boolean;
  }) => invoke<Achievement>("create_achievement", { input }),
  toggleAchievement: (id: string) =>
    invoke<boolean>("toggle_achievement", { id }),
  deleteAchievement: (id: string) =>
    invoke<void>("delete_achievement", { id }),

  // saves
  listSaveProfiles: (gameId: string) =>
    invoke<SaveProfile[]>("list_save_profiles", { gameId }),
  createSaveProfile: (input: {
    game_id: string;
    label: string;
    source_dir: string;
    glob?: string;
    auto_backup?: boolean;
  }) => invoke<SaveProfile>("create_save_profile", { input }),
  backupNow: (profileId: string, note?: string) =>
    invoke<{
      backup_id: number;
      archive_path: string;
      size_bytes: number;
      file_count: number;
    }>("backup_now", { profileId, note }),
  listBackups: (profileId: string) =>
    invoke<SaveBackup[]>("list_backups", { profileId }),
  restoreBackup: (backupId: number) =>
    invoke<void>("restore_backup", { backupId }),

  // playtime
  startSession: (gameId: string, processName?: string) =>
    invoke<number>("start_session", { gameId, processName }),
  stopSession: (gameId: string) =>
    invoke<number | null>("stop_session", { gameId }),
  listSessions: (gameId?: string, limit = 100) =>
    invoke<PlaySession[]>("list_sessions", { gameId, limit }),

  // analytics
  dashboardStats: () => invoke<DashboardStats>("dashboard_stats"),
  heatmap: (days = 365) => invoke<HeatmapCell[]>("heatmap", { days }),
};

export function onEvent(
  handler: (ev: AppEvent) => void
): Promise<UnlistenFn> {
  return listen<AppEvent>("gv://event", (e) => handler(e.payload));
}
