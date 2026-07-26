-- Backfill `playing` for libraries that predate the derived-state rule.
--
-- `completion_state` used to be written only by the user, through the GameDetails
-- tabs. When NOVARA began deriving `playing` from recorded playtime, the rule was
-- applied to sessions ending from then on and never to history, so every game
-- played before that release stayed `unplayed` for ever unless it happened to be
-- played again. A real library showed 341 seconds of City of Gangsters, including
-- a single 201-second session, still labelled Unplayed.
--
-- The state is derived from data already persisted here, so the correction belongs
-- in a one-time migration rather than a runtime repair pass: there is nothing
-- recurring to detect once history has been reconciled. (Contrast the orphaned
-- open-session repair, which runs at every startup because a hard shutdown can
-- always produce a new one.)
--
-- The 60 below is `db::playtime::MIN_PLAYING_SECONDS`. A committed migration is
-- immutable, so this literal cannot follow the constant; the test
-- `threshold_matches_the_backfill_migration` asserts they are equal and fails the
-- build if the constant is ever changed without a deliberate decision about how
-- history should be treated.
--
-- Only `unplayed` rows are touched, so a user's own `completed`, `abandoned`,
-- `backlog` or `playing` classification is preserved exactly.
UPDATE games
SET completion_state = 'playing',
    updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE completion_state = 'unplayed'
  AND total_playtime_seconds >= 60;
