# Miscellaneous Security Hardening

Implementation snapshot: 2026-09-18, `ac33a61d4`, compared directly with the
original 2026-09-04 baseline `23e562215`.

This tracker owns security-sensitive parity work that is not primarily a path
confinement, daemon lifecycle, or TCP framing problem. The current baseline is
the generated rsync 3.5.0 manifests under `tools/ci/` plus the open audit status
in [SECURITY.md](../../../SECURITY.md#upstream-rsync-350-13-aug-2026---triage-in-progress).

## Completion contract

- Safe Rust is evidence against memory corruption, not against state confusion,
  information disclosure, unbounded work, or fail-open behavior.
- For every upstream C memory-safety regression, classify the memory-corruption
  mechanism separately from its observable protocol contract. Test the latter
  whenever a hostile peer can reach it.
- Malformed input must be rejected deterministically with bounded allocation and
  CPU use. It must not be silently accepted into a different protocol state.
- Diagnostics must not disclose content from operator-owned files unless the
  upstream output contract explicitly exposes it.

At this snapshot the filter disclosure, zstd exhaustion, and sender-selftest
rows have moved to `pass`. The only Linux failures left in the committed rsync
3.5.0 manifests are the same four hostile protocol-state rows in each TCP leg.

## P0: Disclosure and resource bounds

- [x] **MS-1 — Stop filter-merge content disclosure.**
  `filter-merge-content-echo` now passes in both Linux pipe legs and both macOS
  pipe legs. Diagnostics retain the operator-named path and line provenance
  without echoing protected rule text.
- [x] **MS-2 — Bound malformed compression parameters.**
  `daemon-zstd-thread-exhaustion` now passes in all four TCP legs. Daemon option
  identities are normalized before refusal and malformed thread parameters are
  rejected before worker allocation. Per-CVE disposition remains part of MS-4.

## P0: Hostile protocol state

- [ ] **MS-3 — Classify and close the protocol-state regressions.**
  `proto-sender-selftest` now passes in all four TCP legs. The remaining rows are
  `proto-cleared-dirflist`, `proto-hlink-gnum`, `proto-msg-info-assert`, and
  `proto-subflist-freed`; they fail identically in Linux and macOS, root and
  non-root TCP manifests. For each, record whether the original C memory fault
  is structurally impossible, then implement the upstream-visible validation,
  exit status, and shutdown behavior. Rust memory safety alone is not a waiver.

## P1: Complete the 3.5.0 audit

- [ ] **MS-4 — Finish per-CVE disposition.** [SECURITY.md](../../../SECURITY.md#upstream-rsync-350-13-aug-2026---triage-in-progress)
  still records only 10 assessed CVEs out of the upstream 33. Give every CVE
  one of `fixed`, `not applicable with evidence`, or an open task in this
  tracker or a linked canonical tracker. Evidence must be a production call
  site plus a focused test or a structural proof. Route confinement, daemon,
  and network items to their owning documents rather than duplicating TODOs
  here.

## Validation

- Run focused parser and multiplex tests with boundary values on both sides of
  every accepted limit.
- Run the rsync 3.5.0 pipe, TCP, and macOS corpus rows named above and regenerate
  their manifests only after the behavior passes against the upstream negative
  control.
- Run `cargo nextest run`; run the existing fuzz targets for touched parsers with
  a recorded finite budget.

## Definition of done

The disclosure, compression, and sender-selftest rows pass. Completion still
requires the four MS-3 rows and an evidence-backed disposition for every 3.5.0
CVE; no task is closed merely by asserting that Rust prevents memory corruption.

Related trackers: [path confinement](PATH-CONFINEMENT.md),
[daemon](DAEMON.md), [TCP](TCP.md), and
[protocol versions](../protocol-versions/PLAN-TODO.md).
