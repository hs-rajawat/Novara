-- Library Integrity System: whether an installation's executable still
-- exists on disk. Free-form TEXT (like games.completion_state) so new
-- states (ignored, archived, offline, ...) don't need a schema change.
ALTER TABLE game_installations ADD COLUMN status TEXT NOT NULL DEFAULT 'installed';
