# ADR-0002: Evidence tiers replace weighted confidence scoring

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Detection
- **Supersedes:** — · **Superseded by:** —

## Context

The first architecture draft combined detection signals probabilistically:

```
score = 1 - Π(1 - wᵢ)
  WriteWitness 0.75 · KbMatch 0.70 · ContentShape 0.45 · NameMatch 0.15–0.35
auto-bind at ≥ 0.85
```

Three problems emerged on review.

The weights were invented. Nothing supported `0.75` over `0.6`, so the model
presented false precision. Retuning one weight silently changes every decision in
the system.

The independence assumption is wrong. Noisy-OR assumes independent evidence, but
`KbMatch` and `NameMatch` are correlated — a KB template usually contains the game's
title — so the formula overstates confidence exactly where conservatism matters most.

And a float cannot be explained or tested. "0.72" means nothing to a user, and a
weighted sum has no natural unit tests.

## Decision

The bind / suggest / discard outcome is decided by an ordered **rule table**,
evaluated top to bottom, first match wins. Each row carries a human-readable
explanation.

A numeric score is still computed, but its only job is ordering the suggestion list.
It never decides anything.

## Alternatives considered

| Option | Why not |
|---|---|
| Keep noisy-OR, tune the weights | The weights have no ground truth to tune against, and the independence assumption stays broken |
| Bayesian model with priors | Same problem one level up: the priors would also be invented. Worth revisiting only with real outcome data |
| Train a classifier | No labelled corpus, and we have committed not to collect one. Also unexplainable, which contradicts a stated design goal |
| Single threshold on a hand-tuned score | What was rejected; this is the same design with fewer signals |
| Always ask the user, never auto-bind | Safest and worst. Auto-binding for high-certainty cases is most of the product value |

## Consequences

- Each rule is a test case; the suite maps one-to-one onto behaviour.
- Every outcome has a sentence: "Changed while you were playing, twice."
- Adding a signal means adding rows, not retuning a global function — changes are
  local and reviewable.
- The conservative bias is explicit: only three rows can auto-bind, and each requires
  observation or curated data. Name similarity alone can never bind.
- Loses the ability to express fine gradations of confidence. Accepted — we do not
  have the data to justify gradations, and pretending otherwise was the original
  error.
- Rule *order* becomes load-bearing and needs its own precedence tests.

## Reopen when

Real outcome data exists — how often each rule was later corrected by a user — to
calibrate against. Collecting it must not violate the privacy commitment, so this is
unlikely soon. The rule table is not a stopgap; it is the better design in the
absence of calibration.

Design: [`GAME_SAVE_DETECTION.md`](../GAME_SAVE_DETECTION.md) §6.
