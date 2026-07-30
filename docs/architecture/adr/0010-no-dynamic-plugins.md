# ADR-0010: No dynamic plugin system

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Parsers, Security
- **Supersedes:** — · **Superseded by:** —

## Context

The original brief asked for a "plugin architecture for parsers", with the reasonable
expectation that community contributors would extend game support without waiting on
the maintainers.

"Plugin" can mean two very different things: a registry of contributions compiled into
the application, or dynamically loaded third-party code (native shared libraries, WASM).
The second is what the word usually implies.

NOVARA's users install games from repacks and scene releases. An application aimed at
that audience defaulting to "load arbitrary native code from the internet" is a poor
security posture regardless of intent.

## Decision

No dynamic plugin loading. Extensibility is provided by declarative manifests
([ADR-0009](./0009-declarative-parsers-first.md)) and compiled in-tree extractors
contributed by pull request.

## Alternatives considered

| Option | Why not |
|---|---|
| Native dynamic libraries (`.dll` / `.so`) | Requires a stable ABI across versions, crash isolation across a process boundary, code signing, revocation, and security review of code that reads user files and writes the database. Permanent cost |
| WASM plugins | Better sandboxing, but still needs a host API surface, a capability model, a distribution channel and a versioning story. Large machinery for a benefit manifests mostly deliver |
| Sandboxed subprocess per plugin | IPC protocol, lifecycle management, timeouts, and a serialisation format — for parsing a JSON file |
| Allow plugins but mark them unsupported | "Unsupported" does not survive contact with users. Crashes and data loss are attributed to NOVARA regardless of the label |
| Do nothing about extensibility | Rejected — that is what ADR-0009 addresses |

## Consequences

- No ABI to stabilise, no sandbox to audit, no revocation mechanism, no third-party
  native code in the process.
- Contributions flow through review, which is slower but means shipped parsers are
  tested and fixture-backed.
- Contributors cannot ship support for a game without either a manifest or a merged PR.
  This is the accepted cost, and manifests make it small for common cases.
- If the manifest format proves insufficient at volume, this decision creates real
  pressure. That pressure is the signal to revisit — see below.

## Reopen when

**All four** hold: a sustained contributor community exists; their formats genuinely
cannot be expressed declaratively; they cannot ship in-tree; and a named person owns
ongoing security review of third-party code. Until then, "submit a manifest" is the
better answer, and it is a better answer to give than an ABI to maintain.

Design: [`PARSER_ARCHITECTURE.md`](../PARSER_ARCHITECTURE.md) §6.
