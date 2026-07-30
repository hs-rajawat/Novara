# ADR-0008: KB signing deferred in favour of per-entry validation

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Knowledge Base, Security
- **Supersedes:** — · **Superseded by:** —

## Context

The community Knowledge Base layer is fetched over the network and steers filesystem
access. Its integrity matters. The instinctive control is a cryptographic signature
over the payload, and the first draft listed signing as a requirement without pricing
it.

Signing is easy to *verify* and hard to *operate*: a keypair with custody, a signing
step in the release pipeline, a rotation story, and a decision about what happens when
verification fails on a user's machine — fail closed and lose all updates, or fail open
and gain nothing.

Meanwhile the community layer is, for the foreseeable future, first-party: published by
the same people who publish the application.

## Decision

The KB is protected by HTTPS transport, a SHA-256 checksum of the payload verified
before apply, strict per-entry validation at import, and size/entry-count bounds.
Detached signatures are deferred.

Per-entry validation is the load-bearing control: template variables must come from the
closed set, absolute paths and `..` are rejected, `role` and `platform` must be known
values.

## Alternatives considered

| Option | Why not |
|---|---|
| Sign the payload now | Key management and failure-mode complexity for a threat that layer separation already bounds. Theatre while the layer is first-party |
| Trust HTTPS alone | No integrity check against a corrupted or truncated download, which is a real and mundane failure |
| TOFU / certificate pinning | Solves transport, not authorship, and adds a rotation problem of its own |
| Ship the community layer in the binary | Defeats the point — the layer exists to update independently of releases |
| Sandbox the KB instead of validating it | Validation *is* the sandbox for data: a closed variable set means a KB entry cannot express a path outside the roots |

## Consequences

- A malicious or corrupted KB is bounded to "suggests useless paths", because it cannot
  express a traversal, cannot write a binding, and cannot touch the user layer.
- No key custody burden, no release-pipeline signing step, no rotation policy.
- We accept that a compromised distribution host could serve bad-but-valid entries.
  The damage is annoyance, and it is correctable by a subsequent refresh.
- Per-entry validation must be genuinely strict and well tested; it is now the only
  meaningful barrier. Malformed-payload rejection is a required test case.
- If the community layer becomes third-party-writable, this ADR must be superseded
  before that ships — not after.

## Reopen when

The community KB accepts entries from parties who are not the application publisher, or
distribution moves to infrastructure we do not control.

Design: [`KNOWLEDGE_BASE.md`](../KNOWLEDGE_BASE.md) §7.
