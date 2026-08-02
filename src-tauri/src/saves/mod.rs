//! Save discovery, resolution and storage.
//!
//! Layered deliberately — see `docs/architecture/SAVE_SYSTEM_ARCHITECTURE.md` §1.
//! Each layer depends only on the one below it, and the single most important rule
//! is that **detection knows nothing about save content, and content handling knows
//! nothing about detection**. They meet at a binding.
//!
//! Present today (Phase 0):
//!
//!   fs      — the [`FileSystem`](fs::FileSystem) abstraction detection reads through
//!   locator — candidate generation from a game title
//!   vault   — backup / restore of a known save folder
//!
//! Planned (see `docs/architecture/IMPLEMENTATION_ROADMAP.md`): `kb`, `verifier`,
//! `witness`, `resolver`.
//!
//! A `SaveService` façade is intended to become the only type commands touch, so a
//! handler cannot reach into `resolver` or `verifier` directly. It is deliberately
//! **not** introduced in Phase 0: it would be a wrapper with no behaviour of its
//! own, and the layer violation it prevents is not yet expressible — there is
//! nothing below `vault` and `locator` for a command to skip past. Revisit at the
//! start of Phase 1.

pub mod backoff;
pub mod bounds;
pub mod evidence;
pub mod fs;
pub mod ignore;
pub mod kb;
pub mod locator;
pub mod pipeline;
pub mod resolver;
pub mod service;
pub mod vault;
pub mod verifier;

/// The declarative test-fixture format and its runner. Test-only: no production
/// code path reads a scenario.
#[cfg(test)]
pub mod scenario;

/// Validation against the developer's own installed library. Test-only, and ignored by
/// default — see the module docs for why it is not part of the normal suite.
#[cfg(test)]
mod real_library_tests;

/// Phase 1 exit-gate invariants (I2, I3, I9, I10) and decision-row coverage.
#[cfg(test)]
mod invariant_tests;
