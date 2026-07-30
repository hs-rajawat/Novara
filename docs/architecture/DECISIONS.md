# Architecture Decisions — moved

Decisions now live as individual Architecture Decision Records:

## → [`adr/README.md`](./adr/README.md)

Rationale for the move: [ADR-0014](./adr/0014-adrs-replace-decision-log.md).

---

## Migration map

Every entry from the former running log, with its new home. The ADRs are not merely
reformatted — each was rewritten to add a systematic *Alternatives considered*
section, which the log entries lacked.

| Former entry | Now |
|---|---|
| Evidence tiers replace probabilistic scoring | [ADR-0002](./adr/0002-evidence-tiers-over-weighted-scoring.md) |
| Bindings allow multiple paths per role | [ADR-0006](./adr/0006-multiple-bindings-per-role.md) |
| The binding store is a system of record, not a cache | [ADR-0007](./adr/0007-binding-is-not-a-cache.md) |
| No dynamic plugin system | [ADR-0010](./adr/0010-no-dynamic-plugins.md) |
| `completion_pct` becomes a derived cache | [ADR-0011](./adr/0011-completion-pct-is-derived.md) |
| KB signing deferred in favour of validation | [ADR-0008](./adr/0008-kb-validation-over-signing.md) |
| Write Witness ships as an mtime sweep first | [ADR-0005](./adr/0005-mtime-sweep-before-watchers.md) |
| Detection is read-only, including the verifier | [ADR-0003](./adr/0003-detection-is-read-only.md) |
| Install directory added to the root set | [ADR-0004](./adr/0004-install-dir-as-candidate-root.md) |
| Detection scenarios are declarative fixtures | [ADR-0013](./adr/0013-scenario-driven-tests.md) |
| Filesystem access behind an injected trait | [ADR-0012](./adr/0012-filesystem-behind-a-trait.md) |

Two decisions were added during the migration, having previously been implicit in the
design documents rather than recorded:

| Added | |
|---|---|
| Write Witness is the primary detection signal | [ADR-0001](./adr/0001-write-witness-as-primary-signal.md) |
| Declarative manifests are the default parser tier | [ADR-0009](./adr/0009-declarative-parsers-first.md) |

This file is a redirect and is not maintained. Do not add decisions here.
