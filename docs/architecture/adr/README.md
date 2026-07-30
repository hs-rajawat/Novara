# Architecture Decision Records

One file per decision. Numbered, immutable once accepted, superseded rather than
edited.

Replaces the single running `DECISIONS.md` — see [ADR-0014](./0014-adrs-replace-decision-log.md).

---

## Index

| # | Decision | Status | Affects |
|---|---|---|---|
| [0001](./0001-write-witness-as-primary-signal.md) | Write Witness is the primary detection signal | Accepted | Detection |
| [0002](./0002-evidence-tiers-over-weighted-scoring.md) | Evidence tiers replace weighted confidence scoring | Accepted | Detection |
| [0003](./0003-detection-is-read-only.md) | Detection is read-only; the verifier reads metadata only | Accepted | Detection, Security |
| [0004](./0004-install-dir-as-candidate-root.md) | The install directory is a candidate root | Accepted | Detection |
| [0005](./0005-mtime-sweep-before-watchers.md) | Ship the Write Witness as an mtime sweep before watchers | Accepted | Detection |
| [0006](./0006-multiple-bindings-per-role.md) | Bindings allow multiple paths per role | Accepted | Detection, Vault |
| [0007](./0007-binding-is-not-a-cache.md) | The binding store is a system of record, not a cache | Accepted | Detection |
| [0008](./0008-kb-validation-over-signing.md) | KB signing deferred in favour of per-entry validation | Accepted | Knowledge Base |
| [0009](./0009-declarative-parsers-first.md) | Declarative manifests are the default parser tier | Accepted | Parsers |
| [0010](./0010-no-dynamic-plugins.md) | No dynamic plugin system | Accepted | Parsers, Security |
| [0011](./0011-completion-pct-is-derived.md) | `completion_pct` is a derived cache with one writer | Accepted | Progress |
| [0012](./0012-filesystem-behind-a-trait.md) | Filesystem access behind an injected trait | Accepted | Detection, Testing |
| [0013](./0013-scenario-driven-tests.md) | Detection tests are declarative scenarios, not per-game Rust | Accepted | Testing |
| [0014](./0014-adrs-replace-decision-log.md) | ADRs replace the running decision log | Accepted | Process |
| [0015](./0015-filesystem-trait-scoped-to-detection.md) | The `FileSystem` trait is scoped to detection, not the whole save system | Accepted | Detection, Testing, Vault |

## Conventions

**Numbering.** Zero-padded four digits, monotonic, never reused — not even for a
withdrawn ADR. The number is a permanent identifier that other documents and commit
messages link to.

**Filename.** `NNNN-kebab-case-summary.md`. The slug should still make sense in five
years; avoid words like "new", "current" or "v2".

**Status.** One of:

| Status | Meaning |
|---|---|
| `Proposed` | Under discussion; may change |
| `Accepted` | In force. **The body is now immutable** |
| `Superseded by ADR-NNNN` | Replaced. Kept because the reasoning is still useful |
| `Withdrawn` | Never adopted. Kept so the question is not reopened blindly |

**Immutability.** Once `Accepted`, the Context, Decision, Alternatives and
Consequences sections are not edited. Changed your mind? Write a new ADR that
supersedes this one, and edit only the `Status` line here. Rewriting history is how
a decision log stops being trustworthy — the whole point is that a reader can see
what was known at the time.

Typo fixes and broken-link repairs are fine.

**Required sections.** Context, Decision, Alternatives considered, Consequences. The
first two say what and why; **Alternatives is the section that earns the format**,
because "we considered X and rejected it because Y" is what stops a contributor
re-proposing X in six months. `Reopen when` is optional but encouraged for
deferrals.

**When to write one.** A decision deserves an ADR if reversing it later would be
expensive, or if a reasonable contributor might propose the opposite. Naming a
variable does not need an ADR. Choosing a storage model does.

**Where the design lives.** ADRs record *decisions*, not designs. The full design
lives in the subsystem documents in `../`; an ADR links to it rather than restating
it. If an ADR is growing sections about schemas and data flow, that content belongs
in a design document.

## Template

Copy [`0000-template.md`](./0000-template.md).
