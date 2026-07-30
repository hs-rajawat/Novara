# ADR-0004: The install directory is a candidate root

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Detection
- **Supersedes:** — · **Superseded by:** —

## Context

The existing detector searches six OS locations: `%APPDATA%`, `%LOCALAPPDATA%`,
`LocalLow`, `Documents`, `Documents/My Games`, and `Saved Games`. It does not search
the game's own install directory.

Portable builds, scene releases and many older titles save beside the executable:

```
Game/
  Game.exe
  save.dat
  config.ini
  saves/
  Profiles/
```

These are exactly the installations NOVARA exists to handle well, and they were
invisible to detection.

## Decision

Each installation's directory (`installations.install_dir`) is a first-class candidate
root, subject to the same depth and count bounds as the OS roots.

The verifier's executable-presence signal is what distinguishes "this install
directory contains saves in a subfolder" from "this is an install directory, not a
save directory".

## Alternatives considered

| Option | Why not |
|---|---|
| Leave it out; portable games are rare | They are not rare in the target population. Omitting the install directory blanks out a large slice of the motivating use case |
| Search the install directory only when OS roots find nothing | Ordering hacks encode assumptions that break. All roots produce candidates; the decision table sorts it out |
| Bind the install root itself when save-like files are present | Too coarse. `{INSTALL}/saves` should bind to `saves`, not to the whole game folder, or a snapshot archives gigabytes of game data |
| Treat the install directory as higher-priority than OS roots | No basis for that; a Steam game with an install-local decoy would mis-bind |

## Consequences

- Portable and scene releases become detectable without a KB entry.
- Raises the risk of binding a game's own data directory, mitigated by the
  executable-presence signal and by preferring the most specific subdirectory.
- Snapshot size becomes a real concern for install-local bindings — a mis-bound
  install root would archive the entire game. The vault should warn above a size
  threshold.
- Install-local candidates alone never auto-bind (decision table rule 8 is a
  *suggest* row); confirmation needs a witness or the user.

Design: [`GAME_SAVE_DETECTION.md`](../GAME_SAVE_DETECTION.md) §7.1.
