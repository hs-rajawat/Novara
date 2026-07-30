# Parser Architecture

**Status:** design. Not implemented. Target: Phase 5 (declarative), Phase 7
(compiled).

How NOVARA reads meaning out of save files. Owns: `parser_manifests`, the
`Extractor` trait, the manifest format, sandboxing rules.

Consumers: [`ACHIEVEMENT_SYSTEM.md`](./ACHIEVEMENT_SYSTEM.md),
[`PROGRESS_TRACKING.md`](./PROGRESS_TRACKING.md). Input contract: a **binding**
from [`GAME_SAVE_DETECTION.md`](./GAME_SAVE_DETECTION.md).

---

## 1. The output: facts, not opinions

An extractor produces `Fact` values and nothing else. It does not write to the
database, does not compute percentages and does not decide what anything means.

```rust
enum Fact {
    AchievementUnlocked { external_id: String, at: Option<String>, inferred_time: bool },
    AchievementDefined  { external_id: String, name: Option<String> },
    Counter             { key: String, value: f64, max: Option<f64> },
    Flag                { key: String, value: bool },
    Enum                { key: String, value: String },
}
```

Keeping extractors this dumb is what makes them safe to accept from contributors
and cheap to test: a test is "these bytes in, these facts out", with no database
and no filesystem.

`inferred_time` matters. Most save formats do not record *when* something was
unlocked. We can infer it from the session window, but we must never present an
inference as an observation — so the flag travels with the fact and reaches the UI.

## 2. Three tiers, in order of preference

| Tier | Form | Ships as | Use when |
|---|---|---|---|
| 1. **Declarative manifest** | Data | KB-style payload, hot-updatable | The format is JSON / INI / XML / key-value — the large majority |
| 2. **Compiled extractor** | Rust trait impl, in-tree | App release | Binary formats, checksums, compression, anything needing real logic |
| 3. **Dynamic plugin** | — | — | **DEFERRED — see §6** |

The ordering is the design. Every game handled by tier 1 costs zero Rust and can
be fixed without a release; every game that drops to tier 2 costs review, tests and
a release cycle. Push work down the tiers.

## 3. Tier 1 — declarative manifests

A manifest describes *where* values live, interpreted by a fixed engine.

```jsonc
{
  "id": "manifest:goldberg-generic",
  "match": { "kind": "any" },
  "version": "1",
  "files": [
    {
      "path": "{save_root}/achievements.json",
      "format": "json",
      "achievements": {
        "iterate": "$.*",             // each key is an achievement id
        "id_from": "key",
        "unlocked_when": "$.earned == true",
        "unlocked_at": "$.earned_time"
      }
    },
    {
      "path": "{save_root}/steam_settings/achievements.json",
      "format": "json",
      "achievements": { "iterate": "$[*]", "id_from": "$.name" }
    }
  ]
}
```

Supported formats: `json`, `ini`, `xml`, `kv` (Valve KeyValues), `csv`. Deliberately
**not** supported: arbitrary binary offsets, regex over binary, executable
expressions. A manifest that needs those is a tier-2 case, and pretending otherwise
turns the manifest engine into a scripting language with a security surface.

Path variables: `{save_root}`, `{install}`, `{steam_userid}`, `{wildcard}` — the
same closed set as the KB, for the same reason.

**Missing files are not errors.** A manifest lists candidate locations; absence is
normal. Only a file that exists and cannot be parsed is a failure worth recording.

### 3.1 Why this covers so much

Emulated-Steam layers converged on a small number of shapes, and modern engines
serialise to JSON or INI. One generic Goldberg manifest plausibly covers hundreds
of games. That leverage is the entire argument for tier 1 existing.

## 4. Tier 2 — compiled extractors

Mirrors `metadata/providers/` exactly, so the codebase has one idiom for
"pluggable thing with capabilities":

```rust
pub trait Extractor {
    fn code(&self) -> &'static str;
    fn supports(&self, ctx: &BindingContext) -> bool;
    fn extract(&self, files: &SnapshotView) -> Lookup<Vec<Fact>>;
}
```

`Lookup<T>` is reused from `resolve/` (see
[`SAVE_SYSTEM_ARCHITECTURE.md`](./SAVE_SYSTEM_ARCHITECTURE.md) §5). The four-way
result is the point: `Unsupported` (not my format), `Temporary` (file locked, retry),
`Permanent` (malformed — stop trying, record it), `Found`.

First implementations: `steam_stats` (`UserGameStats_*.bin`), `goldberg` (variants
the generic manifest cannot express).

## 5. Sandboxing

Extractors read files a stranger may have crafted. Safety comes from the *type*,
not from reviewer diligence:

```rust
pub struct SnapshotView { /* opaque */ }
// - read-only; no write API exists
// - confined to the binding's paths; cannot express a path outside them
// - per-file size cap; per-run total-bytes cap
// - per-run wall-clock budget
// - no network, no process spawning (enforced by review + no API surface)
```

There is no `&Path` and no `File` in the interface. An extractor that wants to
write, or read elsewhere, cannot express it — which is a far stronger guarantee
than a rule in a contributing guide.

Failure isolation: an extractor that panics fails that game only, and the outcome
is recorded with backoff so a pathological file is not re-parsed every session.

## 6. Dynamic plugins — deferred

**Decision: not building this.** Recorded in
[ADR-0010](./adr/0010-no-dynamic-plugins.md).

The costs are permanent: a stable ABI, sandboxing untrusted native code, crash
isolation across a process boundary, a distribution and revocation story, and a
security review of third-party code that reads user files and writes to the
database. For a local-first app whose users install from repacks, "load arbitrary
native code" is a poor default.

The benefits are largely obtainable without it — tier 1 is hot-updatable data, and
tier 2 accepts contributions via PR.

Reopen only if: there is a sustained contributor community, whose formats cannot be
expressed declaratively, and who cannot ship in-tree, *and* someone owns the
security review. Until all four hold, "submit a manifest" is the better answer.

## 7. Execution model

```
session_ended → cold snapshot taken (vault)
      │
      ├─ select extractors: manifests matching the game, then compiled
      │  extractors whose supports() is true
      │
      ├─ run against (previous_snapshot, current_snapshot)
      │     diff by content_hash — unchanged files are skipped entirely
      │
      ├─ facts → FactSink
      │     ├─ achievement facts → ACHIEVEMENT_SYSTEM
      │     └─ counters/flags    → PROGRESS_TRACKING
      │
      └─ record outcome per extractor (success / permanent failure + backoff)
```

Extraction runs **after** the snapshot, never against live files: the game has
exited, the bytes cannot move underneath us, and the diff is free because hashes
already exist.

## 8. Testing

| Level | Approach |
|---|---|
| Manifest engine | Table-driven: fixture file + manifest → expected facts. No filesystem, no DB. |
| Manifest corpus | Every shipped manifest has at least one fixture; a manifest without one is not merged. |
| Compiled extractors | Real captured save files, anonymised, checked in as fixtures |
| Hostile input | Truncated, oversized, deeply nested, wrong-format, zip-bomb fixtures — must fail as `Permanent`, never panic or hang |
| Diff | Unchanged snapshot pair must produce zero facts and zero file reads |

That last one is a real correctness test, not a performance one: an extractor that
re-emits every unlock on every session will make achievement timestamps meaningless.

## 9. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Manifest engine grows into a scripting language | High | Fixed format list; no expressions; tier-2 escape hatch exists precisely so tier 1 can stay dumb |
| Parser rot as games patch | Medium | Manifests are data; failures are `Permanent` with backoff, never fatal |
| Wrong facts corrupt user-visible progress | Medium | Facts carry `source`; parsed state never overwrites user state (see [Achievements §3](./ACHIEVEMENT_SYSTEM.md)) |
| Per-game special cases accumulate | Medium | Tier 1 is the default; a tier-2 PR must justify why declarative was insufficient |
| Extraction slows session end | Low | Diff-gated, capped, and off the UI thread |
