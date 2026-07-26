-- Cache of Steam title searches, so a game that Steam cannot identify is asked
-- about once rather than on every sweep.
--
-- Deliberately narrow. An earlier draft of this table was provider-agnostic
-- (`scheme`/`value` columns able to hold an IGDB or SteamGridDB id too), but
-- nothing needs that: IGDB and SteamGridDB both require an API key and an
-- account, which the project's "no account required" promise rules out, and
-- `epic_catalog` keys off an `AppName` it already has. The generality would have
-- been justified by nothing, and a second narrow table later is a cheaper,
-- better-informed change than a generic one designed against guesses.
--
-- Three states, all unambiguous:
--   * no row          — never searched
--   * app_id IS NULL  — searched, nothing matched (the negative cache; without
--                       it every unmatchable game is re-queried for ever, which
--                       is the defect terminal artwork states exist to prevent)
--   * app_id set      — matched, with the Steam title that matched recorded for
--                       provenance so a wrong match is diagnosable
--
-- `settled_by` is the resolver's fingerprint, mirroring `artwork_assets`: a
-- conclusion is terminal only while the thing that reached it is unchanged, so
-- improving the matcher re-opens past non-matches automatically with no manual
-- repair. It reuses that *mechanism*, not that schema.
--
-- This is not stored on `game_installations.source_app_id`, which records where a
-- game is installed from. A Steam app-id attached to an Epic installation there
-- would corrupt duplicate detection, which keys on (source, source_app_id).
CREATE TABLE steam_title_matches (
    game_id       TEXT PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    app_id        TEXT,
    matched_title TEXT,
    settled_by    TEXT NOT NULL,
    resolved_at   TEXT NOT NULL,
    -- A match must carry its provenance, and a non-match must not claim any:
    -- enforced here because SQLite cannot add a constraint via ALTER later.
    CHECK ((app_id IS NULL) = (matched_title IS NULL))
);
