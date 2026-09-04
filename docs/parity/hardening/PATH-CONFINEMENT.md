# Path Confinement Hardening

Implementation snapshot: 2026-09-18, `ac33a61d4`, compared directly with the
original 2026-09-04 baseline `23e562215`. The documentation commit was rebased,
so its current position is not used to define the review range.

This tracker owns parity work where a peer-controlled or operator-supplied path
can escape its intended root, change identity between validation and use, or
cause a read or write through an unsafe symlink. The generated upstream rsync
3.5.0 result manifests are the source of truth:

- [non-root pipe](../../../tools/ci/upstream-3.5.0-expect.nonroot.txt)
- [root pipe](../../../tools/ci/upstream-3.5.0-expect.root.txt)
- [non-root TCP](../../../tools/ci/upstream-3.5.0-expect.nonroot.tcp.txt)
- [root TCP](../../../tools/ci/upstream-3.5.0-expect.root.tcp.txt)
- [macOS non-root pipe](../../../tools/ci/upstream-3.5.0-expect.macos.nonroot.txt)
- [macOS root pipe](../../../tools/ci/upstream-3.5.0-expect.macos.root.txt)
- [macOS non-root TCP](../../../tools/ci/upstream-3.5.0-expect.macos.nonroot.tcp.txt)
- [macOS root TCP](../../../tools/ci/upstream-3.5.0-expect.macos.root.tcp.txt)

At this snapshot both Linux pipe legs have no failures. Each Linux and macOS
TCP leg has the same four protocol-state failures, and the macOS pipe legs have
two and one environmental failures respectively. Every path-confinement row
named below now passes in every applicable committed manifest.

## Completion contract

- Resolve peer-controlled and operator-supplied paths relative to an already
  opened root. Reject absolute paths, parent traversal, platform prefixes, and
  symlinked intermediate components when that path family is confined.
- Keep validation and use in one handle-relative operation chain. A successful
  preflight followed by an unconfined pathname operation is not complete.
- Never read, disclose, overwrite, rename, or remove an object outside the
  selected destination, module, reference, partial, backup, or restricted-shell
  root.
- Preserve valid upstream behavior for absolute operator paths whose option
  contract explicitly allows them.

## P0: Peer-controlled paths

- [x] **PC-1 — Confine alternate-basis names.**
  `basis-xname-traversal` now passes in all eight manifests. The received xname
  is confined to its basis directory, including the daemon absolute-alt-dest
  path and portable non-Linux walk.
- [x] **PC-2 — Pin partial-basis replacement targets.**
  `malicious-server-partial-basis-symlink-overwrite` now passes in all four TCP
  legs. Partial-basis cleanup and replacement stay on the confined parent.
- [x] **PC-3 — Close daemon module-root escapes.** The root-owned rows
  `daemon-chroot`, `daemon-chroot-munge-default`, `daemon-config-symlink`,
  `daemon-module-private-parent`, and `daemon-symlink-escape-matrix` now pass.
  Per-connection workers prevent chroot state leaking between sessions, while
  the operator-selected root remains distinct from the peer tail across
  privilege dropping and symlink munging.

## P0: Restricted and operator paths

- [ ] **PC-4 — Ship a confined restricted-shell wrapper.** The server-side
  contract exercised through upstream's wrapper is now green:
  `rrsync-backup-dir-inband-pivot` and the other committed `rrsync-*` rows pass.
  The remaining deliverable is an oc-rsync-packaged equivalent of `rrsync` that
  pins the restricted root before startup, covers source, destination, backup,
  partial, and temporary paths, rejects authority-widening options, and forces
  `--drop-D` as required by rsync 3.5.0.
- [x] **PC-5 — Close macOS-only path and race divergences.**
  `daemon-scan-dir-escape`, `operator-path-partial-dir-daemon`,
  `sender-flist-symlink-leak`, and `symlink-race-source` now pass in the macOS
  pipe manifests, and the daemon-applicable rows pass in both macOS TCP
  manifests. The clonefile fast path now inherits the confined source open.

## Validation

- Add focused regression tests for every escape shape before changing the
  corresponding generated manifest row.
- Run the affected focused tests under both root and non-root identities, then
  regenerate all applicable rsync 3.5.0 pipe and TCP manifests.
- Run the macOS corpus for PC-1 and PC-5. A Linux pass does not discharge a
  Darwin-specific path or race task.
- Run `cargo nextest run` after the focused security tests pass.

## Definition of done

All named manifest rows pass at this snapshot. Closing this tracker still
requires packaging and documenting the restricted-shell wrapper, plus the
adversarial wrapper tests in PC-4. Manifest expectations change only in the same
change that fixes and tests the behavior.

Related trackers: [daemon](DAEMON.md), [TCP](TCP.md),
[miscellaneous security](MISC-SECURITY.md), and
[macOS parity](../macos/PLAN-TODO.md).
