-- Save layouts, and a record of library items the scanner declined to import.
--
-- Two independent changes, in one migration because both exist to make a
-- previously-implicit judgement explicit and reviewable.
--   docs/architecture/KNOWLEDGE_BASE.md      (layouts)
--   docs/architecture/LIBRARY_FILTERING.md   (skipped items)
-- ────────────────────────────────────────────────────────────────────

-- ── Save layouts ────────────────────────────────────────────────────
-- A game can already declare several save locations: `save_kb_entries` has no
-- UNIQUE(match_kind, match_value), so N entries may match one game and
-- `kb::candidates` expands all of them. What was missing is *what kind of
-- location* each entry describes.
--
-- Until now the only proxy was `match_kind != 'any'` ("keyed"), which is a
-- matching mechanism, not a classification. The consequence was concrete: a
-- community save layout entered as a keyed builtin entry would satisfy decision
-- table row 5 and bind with exactly the authority of the official path.
--
-- The original schema anticipated this and parked it in free text -- the `note`
-- column's example comment is `'Goldberg builds only'`. This promotes it to
-- something the decision table can read.
--
-- DELIBERATELY NOT A CHECK CONSTRAINT. A new layout kind must be addable as KB
-- *data* -- a corpus update or a community contribution -- without a migration
-- and without a Rust change. `saves::kb::layout` maps a layout to an authority
-- tier and treats anything it does not recognise as the *least* authoritative,
-- so unknown values are safe by construction: the failure mode is
-- under-trusting, never over-trusting.
--
-- Authority is therefore derived from (layer, layout) in code, never declared by
-- the data. `layer` is set by the loader, not the payload, so an entry cannot
-- promote itself. Granting a layout binding authority is a privilege decision
-- and stays a reviewed code change.
ALTER TABLE save_kb_entries
    ADD COLUMN layout TEXT NOT NULL DEFAULT 'unspecified';

-- Lookups never filter by layout -- every applicable layout is evaluated and the
-- evidence model decides between them -- so no index is added. Layout is read
-- from rows already fetched.

-- ── Skipped library items ───────────────────────────────────────────
-- Steam installs system components as ordinary apps: redistributables, runtimes,
-- Proton builds, SDKs. They are not games and must never reach the library UI.
--
-- Recorded rather than discarded, for the same reason detection records its
-- rejections: an item that vanishes with no explanation is indistinguishable
-- from a scanner bug, and "why is my game missing?" needs an answer.
--
-- `override_import` is the storage for a future "Import anyway" action. The
-- column exists now so that adding the UI needs no migration; nothing reads it
-- for a decision yet beyond the scanner honouring it.
CREATE TABLE skipped_library_items (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source_code   TEXT NOT NULL,
    -- The launcher's own id where there is one. Preferred over the title for
    -- identity: an appid is stable, a display name is localised and renameable.
    source_app_id TEXT,
    title         TEXT NOT NULL,
    install_dir   TEXT,
    -- Which filter rule fired, and the sentence explaining it. Both stored so a
    -- skip is explainable without re-running the scan.
    rule          TEXT NOT NULL,
    reason        TEXT NOT NULL,
    -- 0 = keep skipping, 1 = the user asked for it anyway.
    override_import INTEGER NOT NULL DEFAULT 0,
    first_seen_at TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL,

    UNIQUE(source_code, source_app_id, title)
);

CREATE INDEX idx_skipped_override ON skipped_library_items(override_import);
