# macOS Parity Plan

Implementation snapshot: 2026-09-18, `ac33a61d4`, compared directly with the
original 2026-09-04 baseline `23e562215`.

The four committed macOS manifests now record:

| leg | pass | fail | skip |
|---|---:|---:|---:|
| [non-root pipe](../../../tools/ci/upstream-3.5.0-expect.macos.nonroot.txt) | 238 | 2 | 105 |
| [root pipe](../../../tools/ci/upstream-3.5.0-expect.macos.root.txt) | 267 | 1 | 77 |
| [non-root TCP](../../../tools/ci/upstream-3.5.0-expect.macos.nonroot.tcp.txt) | 116 | 4 | 35 |
| [root TCP](../../../tools/ci/upstream-3.5.0-expect.macos.root.tcp.txt) | 132 | 4 | 19 |

The non-root pipe manifest remains explicitly marked as a local-arm64
prediction until a GitHub `macos-latest` artifact replaces or confirms it.

## Current non-pass ownership

| Upstream test | Canonical owner |
|---|---|
| `chmod-setid` | Environmental oracle; upstream 3.5.0 fails identically on the measured non-root host |
| `partial-protected-regular-retry-policy` | Environmental oracle; upstream 3.5.0 fails identically in both measured privilege legs |
| `proto-cleared-dirflist` | [MS-3](../hardening/MISC-SECURITY.md#p0-hostile-protocol-state) |
| `proto-hlink-gnum` | [MS-3](../hardening/MISC-SECURITY.md#p0-hostile-protocol-state) |
| `proto-msg-info-assert` | [MS-3](../hardening/MISC-SECURITY.md#p0-hostile-protocol-state) |
| `proto-subflist-freed` | [MS-3](../hardening/MISC-SECURITY.md#p0-hostile-protocol-state) |

Resolved since the original baseline: `basis-xname-traversal`, `crtimes`,
`daemon-scan-dir-escape`, `filter-merge-content-echo`,
`macos-setgid-ordinary-mode-regression`, `operator-path-partial-dir-daemon`,
`sender-flist-symlink-leak`, and `symlink-race-source` all moved to `pass`.

## P0: Establish the gate

- [ ] **MAC-1 — Confirm the baseline in CI.** All four macOS jobs and manifests
  now exist and run on pull requests, but the non-root pipe manifest still says
  it is an unconfirmed prediction and the contexts are not required. Replace it
  with the emitted CI artifact if outcomes differ, never hand-edit individual
  rows, and have a repository administrator register the macOS contexts as
  required checks.

## P0: macOS-owned behavior

- [x] **MAC-2 — Match set-id mode handling.**
  `macos-setgid-ordinary-mode-regression` passes after applying the same setgid
  mode that `chmod(2)` applies. The remaining non-root `chmod-setid` failure is
  classified environmental because the upstream binary fails identically on
  the measured host; root passes.
- [x] **MAC-3 — Complete creation-time transfer.** `crtimes` passes in both
  pipe manifests after Darwin creation-time preservation was corrected.
- [x] **MAC-4 — Classify protected-regular retry behavior.**
  `partial-protected-regular-retry-policy` remains `fail` in both pipe manifests,
  but the upstream 3.5.0 negative control fails identically. It is an
  environmental oracle, not an oc-rsync implementation divergence; keep the
  row visible so a host-policy change becomes an unexpected pass.

## P1: Platform coverage

- [ ] **MAC-5 — Exercise Darwin-specific storage behavior.** Add coverage for
  default case-insensitive and case-sensitive APFS, resource forks and xattrs,
  ACL inheritance, clonefile fallback, sparse files, symlink metadata, and both
  arm64 and x86_64 execution where runners are available. A skipped capability
  must be reported as such rather than counted as a pass.

## Validation

- Run each macOS-owned upstream test directly before the full corpus.
- Run the focused Rust tests with `cargo nextest run`, then the full suite.
- For metadata cases compare mode, flags, timestamps, ACL/xattr contents, and
  allocation state—not only file bytes.
- Re-run the six current non-pass rows and retain the upstream negative controls
  for the two environmental pipe rows.

## Definition of done

All implementation divergences from the original ten-row pipe baseline are
closed. Completion still requires a CI-confirmed non-root pipe baseline,
required macOS contexts, the cross-platform MS-3 protocol fixes, and the MAC-5
storage matrix. Environmental rows remain recorded rather than relabeled.
