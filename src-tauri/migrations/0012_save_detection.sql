-- ────────────────────────────────────────────────────────────────────
-- Save detection: knowledge base, candidates, scan attempts.
--
-- Phase 1 of the save architecture. See
--   docs/architecture/GAME_SAVE_DETECTION.md  (candidates, evidence, decisions)
--   docs/architecture/KNOWLEDGE_BASE.md       (the KB as a layered asset)
--
-- Deliberately absent: `save_bindings`. Binding is Phase 3, and until there is a
-- store *and* a correction UI, nothing may auto-bind — a wrong binding leads to a
-- wrong restore, and a wrong restore destroys a save. Phase 1 records a decision
-- without acting on it (see `save_candidates.status`).
-- ────────────────────────────────────────────────────────────────────

-- ── Knowledge base ──────────────────────────────────────────────────
-- Replaceable data, never user-specific. Three layers coexist in one table:
-- matching wants all of them at once, so splitting them would mean a three-way
-- union on every lookup. Layer ordering is a CASE in the query.
CREATE TABLE save_kb_entries (
    id            TEXT PRIMARY KEY,       -- layer-prefixed, e.g. 'builtin:steam-userdata'
    layer         TEXT NOT NULL,
    match_kind    TEXT NOT NULL,
    match_value   TEXT NOT NULL,          -- '' when match_kind = 'any'
    platform      TEXT NOT NULL,
    role          TEXT NOT NULL,
    path_template TEXT NOT NULL,          -- '{APPDATA}/{PUBLISHER}/{TITLE}'
    glob          TEXT,
    priority      INTEGER NOT NULL DEFAULT 100,
    note          TEXT,                   -- e.g. 'Goldberg builds only'
    source_ref    TEXT,                   -- provenance: what makes this fixable later
    kb_version    TEXT NOT NULL,
    created_at    TEXT NOT NULL,

    CHECK (layer IN ('builtin', 'community', 'user')),
    CHECK (match_kind IN ('steam_appid', 'gog_id', 'epic_id', 'exe_name', 'title_norm', 'any')),
    CHECK (platform IN ('windows', 'linux', 'macos')),
    CHECK (role IN ('saves', 'config', 'screenshots'))
);

-- The hot path: match a game to entries. Layer is included so the ordering
-- CASE can be satisfied from the index.
CREATE INDEX idx_kb_lookup ON save_kb_entries(match_kind, match_value, platform, layer);
-- Layer replacement deletes by layer, so give it an index of its own.
CREATE INDEX idx_kb_layer ON save_kb_entries(layer);

-- One row per layer. Answers "which KB do you have" independently of the app
-- version, which is the point of versioning the data separately.
CREATE TABLE save_kb_versions (
    layer       TEXT PRIMARY KEY,
    version     TEXT NOT NULL,
    checksum    TEXT NOT NULL,            -- sha256 of the source payload
    entry_count INTEGER NOT NULL,
    applied_at  TEXT NOT NULL,
    source_url  TEXT,                     -- NULL for builtin and user layers

    CHECK (layer IN ('builtin', 'community', 'user'))
);

-- ── Candidates ──────────────────────────────────────────────────────
-- Machine-local, prunable. Evidence is stored rather than only conclusions, so a
-- better algorithm can re-decide the whole corpus offline without touching the
-- filesystem again (GAME_SAVE_DETECTION.md §5.1).
CREATE TABLE save_candidates (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id        TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    path           TEXT NOT NULL,
    role           TEXT NOT NULL DEFAULT 'saves',
    status         TEXT NOT NULL DEFAULT 'candidate',
    -- Ordering and display only. Never decides an outcome: that is the rule table's
    -- job (ADR-0002). Asserting on this value in a test is an anti-pattern.
    score          REAL NOT NULL DEFAULT 0,
    -- Versioned JSON array, append-only. An unknown variant from a newer build
    -- must deserialise without error so a downgrade is survivable.
    evidence_json  TEXT NOT NULL DEFAULT '{"schema":1,"items":[]}',
    -- Which decision-table row produced `status`, for explainability and tests.
    decided_by_rule INTEGER,
    -- The sentence shown to the user. Invariant I9: never empty once decided.
    explanation    TEXT,
    first_seen_at  TEXT NOT NULL,
    last_scored_at TEXT,

    UNIQUE(game_id, path, role),
    CHECK (role IN ('saves', 'config', 'screenshots')),
    -- `bind_eligible` is what Phase 1 records where the decision table says
    -- "bind". Phase 3 converts those into real `save_bindings` rows. The full
    -- vocabulary is fixed now so Phase 3 needs no migration.
    CHECK (status IN ('candidate', 'bind_eligible', 'suggested', 'rejected'))
);

CREATE INDEX idx_candidates_game ON save_candidates(game_id, status);

-- ── Scan attempts ───────────────────────────────────────────────────
-- The genuinely cache-like half of detection: negative results expire, positive
-- results never do (ADR-0007). Mirrors the backoff already proven for artwork in
-- 0007_artwork_backoff.
CREATE TABLE save_scan_attempts (
    game_id       TEXT PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    last_attempt  TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    outcome       TEXT NOT NULL,
    -- NULL means "eligible now". Set after a scan that found nothing, cleared
    -- when new information arrives (a KB refresh, a metadata refresh).
    next_retry_at TEXT,

    CHECK (outcome IN ('bind_eligible', 'suggested', 'nothing', 'error'))
);

CREATE INDEX idx_scan_attempts_retry ON save_scan_attempts(next_retry_at);
