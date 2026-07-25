-- Metadata & Artwork milestone: a fourth artwork kind (logo), plus a
-- per-asset provenance/refresh ledger. `games.*_path` columns remain the
-- render source of truth (frontend reads them directly, no join); this
-- table tracks how/when each asset was obtained and whether refresh may
-- touch it again.
ALTER TABLE games ADD COLUMN logo_path TEXT;

CREATE TABLE artwork_assets (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id      TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    -- Closed sets are enforced here, not just documented: SQLite cannot add
    -- a constraint via ALTER, so a CHECK omitted now costs a full table
    -- rebuild later. `kind` is written only from ArtworkKind::as_str() and
    -- `state` only from the artwork fill loop, so both sets are closed by
    -- construction; `source` is deliberately left free-form because
    -- provider codes are the extension point.
    kind         TEXT NOT NULL CHECK (kind IN ('cover', 'hero', 'logo', 'icon')),
    source       TEXT NOT NULL,                   -- provider code, e.g. 'steam_local'|'steam_cdn'|'epic_catalog'|'manual'
    remote_url   TEXT,                             -- null for local-copy sources
    local_path   TEXT,                             -- absolute path once stored
    -- 'pending' is the lifecycle entry point (no writer inserts it today);
    -- 'ready' and 'skipped' are terminal for the fill loop, 'failed' is
    -- retried on the next sweep.
    state        TEXT NOT NULL DEFAULT 'pending'
                 CHECK (state IN ('pending', 'ready', 'failed', 'skipped')),
    etag         TEXT,                             -- conditional refresh, when the source supports it
    -- 1 = user manually set this asset (set_cover_path/set_hero_path/
    -- set_logo_path/set_icon_path); the auto-fetcher must never overwrite
    -- it, mirroring executable_override in game_installations.
    user_locked  INTEGER NOT NULL DEFAULT 0 CHECK (user_locked IN (0, 1)),
    fetched_at   TEXT,
    updated_at   TEXT NOT NULL,
    -- One row per (game, kind): this table is a slot-ownership ledger, not
    -- an attempt log. The never-clobber guarantee in db::artwork is
    -- expressed *as* this constraint via ON CONFLICT(game_id, kind).
    -- No separate index on game_id alone — the implicit index behind this
    -- UNIQUE has game_id as its leading column and fully serves
    -- `WHERE game_id = ?` (verified with EXPLAIN QUERY PLAN).
    UNIQUE(game_id, kind)
);

-- Privacy-first: automatic metadata/artwork fetching is opt-in, off by
-- default. See offline_mode (0001_init.sql) for the separate global
-- network kill-switch — both must allow network for a provider to fetch.
INSERT INTO settings (key, value) VALUES ('metadata_enabled', 'false');
