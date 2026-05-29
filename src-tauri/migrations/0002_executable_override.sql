-- Track whether the executable path was manually chosen by the user.
-- When executable_override = 1 the scanner will not overwrite the stored value
-- on subsequent rescans, preserving the user's explicit selection.
ALTER TABLE game_installations ADD COLUMN executable_override INTEGER NOT NULL DEFAULT 0;
