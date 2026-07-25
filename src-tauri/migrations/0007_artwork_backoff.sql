-- Persisted retry backoff for the artwork ledger.
--
-- Before this, a `failed` artwork row was retried on every single sweep: the
-- fill loop's only terminal state was `ready`, so a kind no provider could
-- supply (notably `icon`, which no provider offers) kept the whole provider
-- chain running for every game forever — including three CDN HEAD requests per
-- Steam game, per scan, against an undocumented and rate-limited Valve
-- endpoint.
--
-- Two additions, rather than a fixed retry interval in code, so the backoff
-- survives restarts and grows with repeated failure:
--   • `attempts`      — consecutive failures for this (game, kind). Reset to 0
--                       on success, so a slot that recovers is not punished
--                       for its history.
--   • `next_retry_at` — the earliest time this slot may be attempted again.
--                       NULL means "eligible now", which is also the correct
--                       reading for every pre-existing row.
--
-- Terminal states are `ready` and `skipped` (already permitted by 0006's CHECK
-- constraint); `skipped` is what the fill loop now writes for a kind that no
-- provider can supply, which is what finally lets a settled library stop
-- calling providers at all.
ALTER TABLE artwork_assets ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE artwork_assets ADD COLUMN next_retry_at TEXT;

-- Companion validator for conditional refresh.
--
-- `etag` alone is not enough in practice. Measured against the Steam CDN this
-- project actually uses: it returns a strong `ETag`, but answers a matching
-- `If-None-Match` with a full 200 and the entire body, while honouring
-- `If-Modified-Since` with a 304. Storing only the ETag would leave the column
-- populated and the bandwidth saving unrealised, which is the state this batch
-- set out to fix. Both validators are sent, so a refresh is cheap against any
-- origin that honours either.
ALTER TABLE artwork_assets ADD COLUMN last_modified TEXT;

-- Retry eligibility is the hot read of every sweep: one row per (game, kind)
-- filtered by state and due time. The existing UNIQUE(game_id, kind) index
-- serves per-game lookups, but not "which slots are due" across the library.
CREATE INDEX idx_artwork_retry ON artwork_assets(state, next_retry_at);
