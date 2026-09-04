# Protocol Versions 28–32 Parity Plan

Implementation snapshot: 2026-09-18, `ac33a61d4`, compared directly with the
original 2026-09-04 baseline `23e562215`.

This phase covers rsync wire protocols 28 through 32. Protocols 20 through 27
are outside this phase: they are deferred for possible later hardening, not
declared permanent non-goals. Work in this phase must not make their future
implementation harder by assuming protocol 28 is the earliest representable
wire shape inside otherwise version-generic code.

The main interoperability harness still exercises rsync 3.0.9, 3.1.3, 3.4.4,
and 3.5.0. It builds rsync 2.6.9, which advertises protocol 29 and can be forced
to protocol 28. Dedicated 2.6.9 client/daemon cells exist, but the RP28.c/RP28.d
push and pull cells remain `continue-on-error`, and 2.6.9 remains outside the
main `versions=` scenario array.

Protocol-29 keep-alive handling and hardlink-flag decoding improved in the
reviewed range, but they do not close the matrix or ignored-test tasks below.

## Completion contract

- Validate both push and pull with oc-rsync in client and server/daemon roles.
- Test negotiated native versions and explicit `--protocol=N` forcing.
- Compare destination bytes and metadata, wire phase completion, diagnostics,
  and exit status. Destination equality alone does not prove wire-byte parity.
- Keep upstream-imposed old-protocol feature limitations distinct from
  oc-rsync defects.

## P0: Make every supported version a real gate

- [ ] **PV-1 — Gate protocols 28 and 29 against rsync 2.6.9.** Move 2.6.9 from
  build-only coverage into the maintained scenario matrix. Make all four
  client/daemon role combinations exercise push and pull, force protocols 28
  and 29 separately, assert the negotiated version, and remove
  `continue-on-error` only after the complete baseline is green.
- [ ] **PV-2 — Replace the broken server-mode test harness.** Rewrite the seven
  ignored tests in `tests/integration_protocol_versions.rs` so one side is a
  real initiating client instead of piping two `--server` processes together.
  Enable push and pull for protocols 30, 31, and 32 plus protocol-32 delta
  transfer. All seven tests remain ignored. A targeted run at this snapshot
  returned success only because every workspace-anchored upstream binary was
  absent and each test returned early; that is not transfer evidence. Extend
  the repaired harness to 28 and 29 once PV-1 is stable.
- [ ] **PV-3 — Close incremental-recursion sender wire drift.** Make the
  single- and multi-segment tests in
  `tests/inc_recurse_sender_wire_parity_isi_e.rs` byte-identical to the upstream
  sender with fixed capabilities and checksum seed. Preserve destination-tree
  interoperability while correcting file-list segmentation, indices, flags,
  ordering, and phase markers; then remove both ignores.

## P1: Coverage and test debt

- [ ] **PV-4 — Promote stale protocol tests.** Re-run the two ignored
  protocol-28/29 file-list flag tests in
  `crates/protocol/tests/flist_wire_flags_rp28g.rs`; both still pass at this
  snapshot. Confirm the fixtures exercise production encoding, remove their
  ignores, and gate them normally. Audit other protocol ignores so environment-
  gated tests remain opt-in but passing correctness tests do not remain hidden.
- [ ] **PV-5 — Complete the forced-version matrix.** Run protocols 28, 29, 30,
  31, and 32 against both maintained protocol-32 behavior peers, plus the native
  30 and 31 anchor releases. Cover file-list flags, legacy hardlink encoding,
  MD4/MD5/XXH negotiation, legacy and modern filter prefixes, multiplexing,
  goodbye phases, deletion statistics, compression, batch files, ACL/xattr
  rejection below protocol 30, and incremental-recursion gating.

## Expected old-protocol limitations

- ACL and xattr wire fields do not exist before protocol 30.
- Zstd/lz4 choice cannot be negotiated before protocol 30; legacy peers use
  zlib behavior.
- Protocol 28 peers reject modern multi-character filter prefixes.
- Version-scoped upstream batch-reader failures remain recorded as upstream
  limitations and must not be “fixed” by emitting a non-upstream wire format.

The known-failure ledger must scope these by direction, protocol, and upstream
version. It must not use unconditional suppressions that could hide a future
unexpected pass.

## Validation

- Run `bash tools/ci/run_interop.sh` with every maintained peer installed and
  retain its forced 28–32 sweep.
- Run the enabled `integration_protocol_versions` and incremental-recursion
  wire-parity tests with `cargo nextest run`.
- Run wire differential fuzzing with a recorded seed and finite budget for each
  distinct encoding family: 28, 29, 30, and 31/32.
- Cross-check hostile protocol-state failures through the
  [miscellaneous security tracker](../hardening/MISC-SECURITY.md).

## Definition of done

Protocols 28–32 have gating bidirectional interoperability in every supported
role, no correctness test remains ignored because of broken infrastructure,
and every accepted version has both semantic-transfer and wire-level evidence.
