// Mirrors the Rust models exactly. Keep in sync with src-tauri/src/models.rs.
//
// `primary_source_code`/`primary_source_label` aren't columns on the `games`
// table — they're resolved server-side (src-tauri/src/commands/games.rs,
// `GameSummary`/`GameWithInstalls`) from the game's primary installation and
// flattened onto every `list_games`/`get_game` response, so they're safe to
// treat as part of `Game` on the frontend even though `Db::get_game`'s raw
// row doesn't carry them.
export interface Game {
  id: string;
  title: string;
  sort_title: string;
  description: string | null;
  release_year: number | null;
  developer: string | null;
  publisher: string | null;
  cover_path: string | null;
  hero_path: string | null;
  icon_path: string | null;
  metadata_json: string | null;
  metadata_source: string | null;
  is_favorite: number;
  is_hidden: number;
  completion_pct: number;
  completion_state: CompletionState;
  user_rating: number | null;
  user_notes: string | null;
  total_playtime_seconds: number;
  last_played_at: string | null;
  added_at: string;
  updated_at: string;
  primary_source_code: string | null;
  primary_source_label: string | null;
  /** Library Integrity status ("installed" | "missing") of the primary installation, if any. */
  primary_install_status: string | null;
}

export type CompletionState =
  | "unplayed"
  | "playing"
  | "completed"
  | "abandoned"
  | "backlog";

export interface Installation {
  id: string;
  game_id: string;
  source_id: number;
  install_dir: string;
  executable: string | null;
  launch_args: string | null;
  source_app_id: string | null;
  install_size_bytes: number | null;
  is_primary: number;
  detected_at: string;
  /** 1 when the executable was manually chosen; scanner will not overwrite it. */
  executable_override: number;
  /** Library Integrity System status: "installed" | "missing" | (future user-asserted states). */
  status: string;
  last_verified_at: string | null;
}

export interface GameWithInstalls extends Game {
  installations: Installation[];
}

export interface Achievement {
  id: string;
  game_id: string;
  template_id: string | null;
  name: string;
  description: string | null;
  category: string | null;
  icon_path: string | null;
  points: number;
  is_secret: number;
  is_unlocked: number;
  unlocked_at: string | null;
  sort_order: number;
}

export interface SaveProfile {
  id: string;
  game_id: string;
  label: string;
  source_dir: string;
  glob: string | null;
  auto_backup: number;
  created_at: string;
  /** 1 when the user manually chose this folder; 0 = auto-detected. */
  is_manual_override: number;
}

export interface DetectedSavePath {
  path: string;
  /** 0.0 – 1.0, higher = more likely correct. */
  confidence: number;
  /** Human-readable hint about which heuristic matched. */
  hint: string;
}

export interface SaveBackup {
  id: number;
  profile_id: string;
  archive_path: string;
  size_bytes: number;
  file_count: number;
  note: string | null;
  created_at: string;
}

export interface PlaySession {
  id: number;
  game_id: string;
  started_at: string;
  ended_at: string | null;
  duration_seconds: number;
  idle_seconds: number;
  process_name: string | null;
}

export interface ScanReport {
  source: string;
  found: number;
  added: number;
}

export interface DashboardStats {
  total_games: number;
  completed_games: number;
  total_playtime_seconds: number;
  favorite_count: number;
  recently_played: RecentGame[];
  top_genres: { name: string; count: number }[];
}

export interface RecentGame {
  id: string;
  title: string;
  cover_path: string | null;
  last_played_at: string | null;
  total_playtime_seconds: number;
}

export interface HeatmapCell {
  day: string;
  seconds: number;
}

export interface AppEvent {
  type:
    | "scan_started"
    | "scan_finished"
    | "game_added"
    | "game_updated"
    | "achievement_unlocked"
    | "session_started"
    | "session_ended"
    | "save_backup_created"
    | "notice";
  // intentionally permissive — discriminated payloads vary
  [key: string]: unknown;
}
