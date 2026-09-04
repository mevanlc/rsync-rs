# Windows Parity Plan

Implementation snapshot: 2026-09-18, `ac33a61d4`, compared directly with the
original 2026-09-04 baseline `23e562215`.

Windows parity means native Windows behavior where the platform has an
equivalent and deliberate emulation where rsync depends on Unix shell or
filesystem behavior that Windows does not provide. NTFS Alternate Data Streams
and `--fake-super` are the compatibility carrier for POSIX metadata that cannot
be represented natively.

Production source is authoritative. Older Windows matrices describe several
items as pending even though extended-length paths, reparse-point
classification, bounded chunked reads, SDDL transport, and NTFS sparse marking
now have production call sites; this plan does not reopen them without a failing
test against the current implementation.

No WIN-1 through WIN-9 item closed in the reviewed range. A current source
check still finds literal local wildcard expansion absent, the Windows
`HardLinkTracker` methods as no-ops, cloud placeholders without a transfer
policy, and the three Windows ACL controls documented as planned.

## Completion contract

- Preserve representable Windows metadata natively and unrepresentable POSIX
  metadata through a documented emulation format when requested.
- Never silently discard a requested entry or metadata class. Use a successful
  emulated round trip or a precise warning and partial-transfer status.
- Test on real NTFS in addition to compile-checking `cfg(windows)` paths.
- Preserve the rsync wire protocol. Windows-only fidelity data must use
  negotiated existing metadata carriers rather than unversioned wire fields.

## P0: Command-line and filesystem semantics

- [ ] **WIN-1 — Expand local wildcard operands on native Windows.** When a
  local source operand reaches oc-rsync containing wildcard metacharacters,
  expand it before transfer planning so both `oc-rsync *.md remote:.` and
  `oc-rsync "*.md" remote:.` work under shells that pass the pattern literally,
  following Microsoft OpenSSH `scp.exe` as the behavioral precedent. Expansion
  applies only to local source operands, not options, destinations, or remote
  `host:path` operands. Sort matches deterministically; preserve multiple source
  operand order; and pass an unmatched pattern through to the normal
  missing-source diagnostic and exit status.
- [ ] **WIN-2 — Detect source hardlink cohorts.** Replace the no-op Windows
  `HardLinkTracker::existing_target` and `record` implementation with NTFS file
  identity tracking using volume identity plus file ID. Preserve hardlinks for
  local copy and sender paths, never coalesce equal-content files that are not
  links, and handle cross-volume and unsupported-filesystem fallbacks.
- [ ] **WIN-3 — Detect case-colliding destination names.** Before mutation,
  detect source entries that resolve to one name on a case-insensitive target.
  Report all colliding source names and fail without nondeterministically
  overwriting one of them. Cover case-sensitive Windows directories and remote
  Windows destinations as separate cases.
- [ ] **WIN-4 — Define cloud-placeholder transfer behavior.** The reparse
  classifier already identifies OneDrive/Cloud Files entries, but the sender
  currently emits an empty symlink target. Replace that placeholder behavior
  with an explicit policy: transfer hydrated file contents when available;
  otherwise preserve supported reparse metadata through the Windows fidelity
  carrier or warn and return partial status without creating a misleading
  symlink.

## P0: Metadata fidelity

- [ ] **WIN-5 — Complete POSIX metadata emulation.** Under `--fake-super`,
  round-trip mode, uid, gid, device numbers, FIFOs, sockets, and device entries
  through NTFS ADS in the upstream-compatible representation. Without
  emulation, apply the representable read-only/DACL subset and report every
  unrepresentable requested class. Define `--usermap`, `--groupmap`, and
  `--chown` behavior over stored POSIX identities without pretending Windows
  SIDs are numeric Unix IDs.
- [ ] **WIN-6 — Make ACL loss explicit and controllable.** Preserve complete
  Windows-to-Windows SDDL, including deny and inherited ACEs, inheritance state,
  owner/group, SACL when authorized, non-`rwx` rights, and unresolved SIDs.
  Finish the documented `--windows-acls`, `--audit-acls`, and
  `--fail-on-windows-acl-loss` interfaces. Cross-platform transfers may use the
  lossy POSIX ACL mapping only when the loss is reported according to those
  controls.

## P1: Runtime integration and coverage

- [ ] **WIN-7 — Select the production Windows reader deliberately.** The IOCP
  reader and factory exist and have parity tests, but the documented production
  sender path still uses standard buffered file reads. Wire IOCP reads into the
  production selection without depending on the off-by-default experimental
  `adaptive-basis-dispatch`; retain deterministic buffered fallback and prove
  bounded RSS and byte-identical output across local, SSH, and daemon transfers.
- [ ] **WIN-8 — Add service-safe config reload.** Provide the Windows equivalent
  of SIGHUP through a named event or service control path, authenticate access
  to the signal, and atomically retain the old configuration when reload fails.
- [ ] **WIN-9 — Close the validation matrix.** x86_64 MSVC has required and
  nightly IOCP, ACL/xattr, and OpenSSH coverage. Add ARM64 Windows build and test
  coverage, make the real-NTFS evidence explicit, and close the remaining ADS,
  DACL/SDDL, hardlink, junction, file-symlink privilege, long-path, sparse,
  case-collision, daemon, and OpenSSH matrix cells.

## Platform limits

- Native Windows cannot create Unix device nodes, FIFOs, or socket inodes;
  parity for those entries is ADS/`--fake-super` emulation plus explicit
  diagnostics when emulation is not requested.
- Directory symlinks may fall back to junctions. File symlinks require
  Developer Mode or the appropriate privilege and have no equivalent junction
  fallback.
- Linux-specific APIs such as io_uring and Landlock are not Windows parity
  requirements; their Windows security or performance outcomes are.

## Validation

- Unit-test operand expansion by passing literal argv values, independent of
  PowerShell or cmd.exe preprocessing. Add end-to-end local and remote wildcard
  cases in both shells.
- Run focused Windows nextest packages, all Windows nightly jobs, and physical
  NTFS round trips for metadata-sensitive tasks.
- Compare destination trees, link/file identities, ADS bytes, SDDL, allocated
  ranges, diagnostics, and exit status rather than checking only command
  success.

## Definition of done

All P0 tasks have real-NTFS evidence, unsupported POSIX constructs round-trip
through emulation or fail visibly, and both x86_64 and ARM64 Windows builds are
covered. P1 performance work must retain a tested buffered fallback.
