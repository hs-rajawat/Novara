-- GameVault schema v1
-- Local-first SQLite. All times stored as RFC3339 strings (sqlx-chrono compat).

PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

-- ────────────────────────────────────────────────────────────────────
-- Source: how a game was discovered (Steam, Epic, Manual, Emulator…)
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE sources (
    id            INTEGER PRIMARY KEY,
    code          TEXT NOT NULL UNIQUE,         -- 'steam' | 'epic' | 'gog' | ...
    display_name  TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1
);

INSERT INTO sources (code, display_name) VALUES
    ('steam',   'Steam'),
    ('epic',    'Epic Games'),
    ('gog',     'GOG'),
    ('xbox',    'Xbox App'),
    ('ubisoft', 'Ubisoft Connect'),
    ('battle',  'Battle.net'),
    ('emulator','Emulator ROM'),
    ('manual',  'Manual / Other');

-- ────────────────────────────────────────────────────────────────────
-- Games: one row per unique title (duplicates merged into one game,
-- multiple installations attached via game_installations)
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE games (
    id              TEXT PRIMARY KEY,             -- uuid v4
    title           TEXT NOT NULL,
    sort_title      TEXT NOT NULL,                -- normalized for sorting (strip "The ")
    description     TEXT,
    release_year    INTEGER,
    developer       TEXT,
    publisher       TEXT,
    cover_path      TEXT,                         -- local asset path
    hero_path       TEXT,                         -- banner / splash
    icon_path       TEXT,
    metadata_json   TEXT,                         -- raw metadata blob (IGDB/RAWG)
    metadata_source TEXT,                         -- 'igdb' | 'rawg' | 'manual'

    is_favorite     INTEGER NOT NULL DEFAULT 0,
    is_hidden       INTEGER NOT NULL DEFAULT 0,
    completion_pct  REAL NOT NULL DEFAULT 0,      -- 0..100
    completion_state TEXT NOT NULL DEFAULT 'unplayed', -- unplayed|playing|completed|abandoned|backlog
    user_rating     INTEGER,                      -- 1..10 nullable
    user_notes      TEXT,

    total_playtime_seconds INTEGER NOT NULL DEFAULT 0,
    last_played_at  TEXT,
    added_at        TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX idx_games_sort_title    ON games(sort_title);
CREATE INDEX idx_games_last_played   ON games(last_played_at);
CREATE INDEX idx_games_favorite      ON games(is_favorite) WHERE is_favorite = 1;

-- ────────────────────────────────────────────────────────────────────
-- Genres (many-to-many)
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE genres (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE
);

CREATE TABLE game_genres (
    game_id  TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    genre_id INTEGER NOT NULL REFERENCES genres(id) ON DELETE CASCADE,
    PRIMARY KEY (game_id, genre_id)
);

-- ────────────────────────────────────────────────────────────────────
-- Installations: a game can be installed multiple times (multiple
-- drives, multiple sources). The launcher uses the "primary" one.
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE game_installations (
    id             TEXT PRIMARY KEY,             -- uuid
    game_id        TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    source_id      INTEGER NOT NULL REFERENCES sources(id),
    install_dir    TEXT NOT NULL,
    executable     TEXT,                         -- relative to install_dir
    launch_args    TEXT,
    source_app_id  TEXT,                         -- steam appid, epic catalog id, etc
    install_size_bytes INTEGER,
    is_primary     INTEGER NOT NULL DEFAULT 1,
    detected_at    TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_install_dir ON game_installations(install_dir);
CREATE INDEX idx_install_game ON game_installations(game_id);
CREATE INDEX idx_install_source_appid ON game_installations(source_id, source_app_id);

-- ────────────────────────────────────────────────────────────────────
-- Playtime sessions (raw events; aggregated into games.total_playtime)
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE play_sessions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id     TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    started_at  TEXT NOT NULL,
    ended_at    TEXT,
    duration_seconds INTEGER NOT NULL DEFAULT 0,
    idle_seconds INTEGER NOT NULL DEFAULT 0,
    process_name TEXT
);

CREATE INDEX idx_sessions_game ON play_sessions(game_id, started_at);
CREATE INDEX idx_sessions_started ON play_sessions(started_at);

-- ────────────────────────────────────────────────────────────────────
-- Achievements (custom + community templates)
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE achievements (
    id           TEXT PRIMARY KEY,                -- uuid
    game_id      TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    template_id  TEXT,                            -- if from a community template
    name         TEXT NOT NULL,
    description  TEXT,
    category     TEXT,                            -- 'main'|'side'|'collectible'|'ending'|'custom'
    icon_path    TEXT,
    points       INTEGER NOT NULL DEFAULT 10,
    is_secret    INTEGER NOT NULL DEFAULT 0,
    is_unlocked  INTEGER NOT NULL DEFAULT 0,
    unlocked_at  TEXT,
    sort_order   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_ach_game ON achievements(game_id, sort_order);
CREATE INDEX idx_ach_unlocked ON achievements(game_id, is_unlocked);

CREATE TABLE achievement_templates (
    id           TEXT PRIMARY KEY,                -- shareable id
    game_title   TEXT NOT NULL,                   -- fuzzy match to games.title
    name         TEXT NOT NULL,
    description  TEXT,
    category     TEXT,
    points       INTEGER NOT NULL DEFAULT 10,
    author       TEXT,
    imported_at  TEXT NOT NULL
);

-- ────────────────────────────────────────────────────────────────────
-- Save backups (versioned snapshots of save folders)
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE save_profiles (
    id         TEXT PRIMARY KEY,
    game_id    TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    label      TEXT NOT NULL,                    -- e.g. "Cloud", "Documents/MyGames/..."
    source_dir TEXT NOT NULL,                    -- absolute path watched
    glob       TEXT,                             -- optional include filter
    auto_backup INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE TABLE save_backups (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id   TEXT NOT NULL REFERENCES save_profiles(id) ON DELETE CASCADE,
    archive_path TEXT NOT NULL,                  -- zip in app data
    size_bytes   INTEGER NOT NULL,
    file_count   INTEGER NOT NULL,
    note         TEXT,
    created_at   TEXT NOT NULL
);

CREATE INDEX idx_backups_profile ON save_backups(profile_id, created_at);

-- ────────────────────────────────────────────────────────────────────
-- Screenshots & clips
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE media (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id     TEXT REFERENCES games(id) ON DELETE SET NULL,
    kind        TEXT NOT NULL,                   -- 'screenshot'|'clip'|'cover'|'hero'
    path        TEXT NOT NULL UNIQUE,
    captured_at TEXT,
    width       INTEGER,
    height      INTEGER,
    bytes       INTEGER,
    tags_json   TEXT                             -- ["funny","boss"]
);

CREATE INDEX idx_media_game ON media(game_id, captured_at);

-- ────────────────────────────────────────────────────────────────────
-- Mods
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE mods (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id       TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    path          TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    load_order    INTEGER NOT NULL DEFAULT 0,
    version       TEXT,
    author        TEXT,
    compat_notes  TEXT,
    added_at      TEXT NOT NULL
);

CREATE INDEX idx_mods_game ON mods(game_id, load_order);

-- ────────────────────────────────────────────────────────────────────
-- User profiles (multi-user)
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE profiles (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    avatar_path TEXT,
    is_active   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

INSERT INTO profiles (id, name, is_active, created_at) VALUES
    ('00000000-0000-0000-0000-000000000001', 'Default', 1, datetime('now'));

-- ────────────────────────────────────────────────────────────────────
-- Settings (key/value, JSON-encoded values)
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO settings (key, value) VALUES
    ('theme', '"dark"'),
    ('telemetry_enabled', 'false'),
    ('offline_mode', 'false'),
    ('scan_paths', '[]'),
    ('idle_threshold_seconds', '300'),
    ('schema_version', '1');

-- ────────────────────────────────────────────────────────────────────
-- Scan history (auditing what the scanner found, for debugging)
-- ────────────────────────────────────────────────────────────────────
CREATE TABLE scan_runs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_code TEXT NOT NULL,
    started_at  TEXT NOT NULL,
    finished_at TEXT,
    found_count INTEGER NOT NULL DEFAULT 0,
    added_count INTEGER NOT NULL DEFAULT 0,
    error       TEXT
);
