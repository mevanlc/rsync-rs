# Daemon Hardening

Implementation snapshot: 2026-09-18, `ac33a61d4`, compared directly with the
original 2026-09-04 baseline `23e562215`.

This tracker owns rsync-daemon behavior that is independent of whether the
session uses a stdio pipe or a TCP socket. TCP framing and PROXY protocol work
lives in [TCP.md](TCP.md); filesystem boundary work lives in
[PATH-CONFINEMENT.md](PATH-CONFINEMENT.md).

## Completion contract

- Apply configuration and option limits before allocating unbounded memory,
  launching helpers, changing privilege, or entering the transfer protocol.
- Parse daemon-delivered arguments with the same option identity, aliases,
  refusal rules, and exit behavior as rsync 3.5.0.
- Keep standalone process behavior deterministic in foreground and detach modes.
- Run privilege-sensitive cases in both root and non-root test legs.

All five daemon-owned rows now pass in every applicable committed manifest.

## P0: Argument and helper handling

- [x] **DM-1 — Enforce daemon argv limits.** `daemon-argv-limit` passes in all
  four TCP manifests. Argument count and encoded size are bounded before the
  transfer configuration is built.
- [x] **DM-2 — Reject invalid name-converter output.**
  `daemon-namecvt-newline-token` passes in all four TCP manifests. Invalid or
  refused converter tokens are reported rather than swallowed or reinterpreted.
- [x] **DM-3 — Normalize refused compression aliases.**
  `daemon-refuse-compress-threads-alias` passes in all four TCP manifests after
  aliases and negated forms are resolved to their canonical option identity.

## P1: Process and permission behavior

- [x] **DM-4 — Match standalone detach semantics.**
  `daemon-standalone-detach` passes in all four TCP manifests. The accept path
  now uses a poll-based parent and per-connection workers with explicit reaping.
- [x] **DM-5 — Match restrictive non-root permissions.**
  `nonroot-restrictive-perms` passes in both root TCP manifests while preserving
  the intended mode across daemon identity changes.

## Cross-owned daemon failures

- Chroot, module-root, config-symlink, and symlink-escape rows are owned by
  [path confinement](PATH-CONFINEMENT.md); all named manifest rows pass, while
  packaging an oc-rsync restricted-shell wrapper remains open.
- Zstd thread exhaustion now passes. The four remaining hostile protocol-state
  rows are owned by [miscellaneous security](MISC-SECURITY.md).
- Handshake timeout, PROXY protocol, and request/response framing are closed in
  [TCP hardening](TCP.md).
- The daemon remaining plaintext is not a parity gap; upstream rsync also relies
  on SSH or an external TLS tunnel for encryption.

## Validation

- Add focused tests for each accepted boundary and its first rejected value.
- Run daemon tests over both the stdio-pipe entry point and loopback TCP wherever
  transport should not affect the result.
- Regenerate the root and non-root TCP manifests only when the corresponding
  upstream 3.5.0 row passes.
- Run `cargo nextest run -E 'deps(daemon)'`, followed by the full suite after
  focused validation.

## Definition of done

Satisfied for DM-1 through DM-5 at this snapshot: all daemon-owned rows pass in
their applicable privilege legs. Keep the focused limits and process-lifecycle
tests as regression gates.
