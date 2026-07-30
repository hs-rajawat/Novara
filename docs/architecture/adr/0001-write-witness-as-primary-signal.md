# ADR-0001: Write Witness is the primary detection signal

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Detection
- **Supersedes:** — · **Superseded by:** —

## Context

Manually installed games have no reliable save-location convention. The same title
saves in `%APPDATA%`, `%LOCALAPPDATA%`, `Documents/My Games`, `Saved Games` or beside
the executable depending on how it was installed — Goldberg, CODEX, RUNE, a repack, a
portable build. Searching the web typically returns three plausible answers, one of
which is right for one release.

The existing detector (`save_detect.rs`) generates title variants and matches them
against directory names. It is guessing, and its `confidence` value measures name
similarity rather than the probability that a directory holds saves.

NOVARA already records exact play-session intervals in `play_sessions`.

## Decision

Detection treats **filesystem writes observed during a play session** as its primary
signal. Name matching is demoted to the weakest evidence type, used for cold-start
candidate generation only.

A directory written while the game was running, inside a bounded root set and not on
the ignore list, is treated as near-conclusive after two sessions.

## Alternatives considered

| Option | Why not |
|---|---|
| Better alias generation and fuzzy matching | Improves a fundamentally weak signal. Name similarity does not distinguish `Documents/Photos` (a game) from `Documents/Photos` (photographs) |
| A large curated path database as the primary mechanism | Necessary but insufficient — it cannot cover unknown repacks, and it describes the *typical* install rather than this machine. Retained as a secondary signal |
| Ask the user to pick the folder every time | The problem statement explicitly rejects this. It is also the status quo and it does not scale to thousands of games |
| Scan the whole disk for save-shaped folders | Minutes of I/O, blamed on NOVARA, and still ambiguous. Rejected — see bounded roots in the detection design |
| Read process file handles while the game runs | Far more invasive, platform-specific, requires elevated privileges, and antivirus-hostile. The session boundary gives most of the signal for none of the cost |

## Consequences

- Detection improves *by itself* as the user plays, with no KB coverage required.
- Works identically for any release group, launcher or era — the mechanism is
  indifferent to how the game was installed.
- Cold start is weaker: a game never launched under NOVARA gets suggestions, not a
  binding. Accepted, because a wrong binding is worse than no binding.
- Creates a dependency from detection onto the playtime subsystem's session events.
- Requires the ignore list (`Logs`, `Crashes`, `GPUCache`, …) to be genuinely good;
  without it the witness records noise. This is the main ongoing maintenance cost.
- Concurrent sessions make attribution ambiguous and need explicit handling.

Design: [`GAME_SAVE_DETECTION.md`](../GAME_SAVE_DETECTION.md) §10.
