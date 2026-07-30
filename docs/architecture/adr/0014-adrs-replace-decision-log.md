# ADR-0014: ADRs replace the running decision log

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Process
- **Supersedes:** — · **Superseded by:** —

## Context

Architectural decisions were first recorded in a single `DECISIONS.md`, appended
newest-first, with each entry giving what was decided, what it replaced, why, and
sometimes what would reopen it.

At eleven entries it worked. Three problems were already visible.

Cross-references were fragile: other documents linked to
`DECISIONS.md#2026-07-30-evidence-tiers-replace-probabilistic-scoring`. Markdown anchor
generation varies between renderers, and editing a heading silently breaks every inbound
link.

Nothing enforced immutability. A single editable file invites revising an entry in place,
which destroys the record of what was known at the time — the only reason the log has
value.

And the entries lacked a systematic *Alternatives considered* section. They explained
what changed and why, but not what else was on the table, which is exactly the question a
contributor asks six months later before re-proposing a rejected option.

## Decision

One file per decision under `docs/architecture/adr/`, numbered `NNNN`, with required
sections: Context, Decision, Alternatives considered, Consequences.

Accepted ADRs are immutable. A changed decision is a **new ADR that supersedes** the old
one; only the `Status` line of the superseded record is edited.

`DECISIONS.md` becomes a pointer to the index, retaining a mapping from its former
entries to ADR numbers so existing links still land somewhere useful.

## Alternatives considered

| Option | Why not |
|---|---|
| Keep the single running log | Fragile anchors, no immutability pressure, and a file that grows past useful browsing. Fine at 11 entries, poor at 60 |
| Keep the log and add ADRs for big decisions only | Two sources of truth for the same class of information, and an unresolvable argument about which decisions are "big" |
| GitHub issues or a wiki for decisions | Not versioned with the code, not available offline, and lost if the host changes. Decisions should ship in the repository |
| Record decisions only in commit messages | Undiscoverable. Nobody greps history to find out why plugins were rejected |
| Migrate later, once there are more entries | Migration cost grows monotonically: eleven entries and nine inbound links today, sixty entries and dozens of links later. Now is the cheapest it will ever be |

## Consequences

- Stable link targets: `adr/0002-evidence-tiers-over-weighted-scoring.md` does not break
  when a heading is reworded.
- The *Alternatives considered* section is now mandatory, which is the substantive
  upgrade — each ADR was rewritten to fill it rather than merely reformatted.
- Supersession is explicit and visible, so the history of a decision is readable.
- More files. Discoverability now depends on the index being maintained; a new ADR that
  is not indexed is effectively invisible.
- A small amount of ceremony per decision. Acceptable, and the template keeps it to a
  few minutes.
- Contributors get an obvious place to propose a decision (`Status: Proposed`) rather
  than arguing in a pull-request thread.

Conventions: [`adr/README.md`](./README.md).
