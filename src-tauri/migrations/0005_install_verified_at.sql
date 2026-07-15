-- Last time the Library Integrity System confirmed this installation's
-- status. Nullable — never verified yet is a valid, common state.
ALTER TABLE game_installations ADD COLUMN last_verified_at TEXT;
