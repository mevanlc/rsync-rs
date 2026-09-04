# Edge-Case Compatibility Plan

Implementation snapshot: 2026-09-18, `ac33a61d4`, compared directly with the
original 2026-09-04 baseline `23e562215`.

This tracker owns confirmed rsync behavior gaps that are not specific to a
protocol version, security boundary, or operating system. The ignored tests
listed below were re-run with `cargo nextest run --run-ignored only` at this
snapshot. All nine behavior tests in EC-1 through EC-8 still fail for the stated
reason. The two EC-9 tests still pass while ignored.

## Completion contract

- Match upstream observable behavior: selected paths, destination contents,
  diagnostics, itemization, and exit status.
- Exercise local and remote transfer paths when they have separate planners or
  executors.
- Remove an `#[ignore]` only after the test passes for the intended reason and
  remains deterministic under normal `cargo nextest run` execution.
- Do not retain tests that assert obsolete behavior merely because their ignore
  annotation is stale.

## P0: Selection and traversal semantics

- [ ] **EC-1 — Complete `--no-implied-dirs`.** Make
  `no_implied_dirs_does_not_create_intermediate_directories` pass. Do not create
  an unlisted parent merely to make a deep operand transferable; preserve
  explicitly listed parent directories and relative-path semantics.
- [ ] **EC-2 — Exempt non-regular entries from size filters.** Make
  `min_size_does_not_affect_symlinks` pass and cover the corresponding
  `--max-size` behavior. Size filters select regular-file payloads, not symlink,
  directory, device, or special-file entries.
- [ ] **EC-3 — Fix non-recursive trailing-slash listing.** Make
  `list_only_without_recursive_shows_only_top_level` emit each immediate child
  exactly once without descending, copying, or changing the destination.
- [ ] **EC-4 — Recover from symlink loops.** Make
  `walk_detects_direct_symlink_loop` and `error_recovery_symlink_loop` pass.
  Under link-following modes, diagnose and skip the loop, continue transferring
  ordinary files, avoid hangs and recursion exhaustion, and return the upstream
  partial-transfer status where applicable.
- [ ] **EC-5 — Finish directory-merge exclusion scope.** Make
  `exclude_lsh_leg_6_dir_merge_excl_restricted` pass. A nested merge file may
  re-include an entry within its scope without corrupting parent-rule ordering,
  inheritance, deletion protection, or sender/receiver modifiers.

## P1: Output and operational wiring

- [ ] **EC-6 — Emit `SYMSAFE` notices.** Make
  `copy_unsafe_links_emits_info_symsafe_notice` emit one correctly routed notice
  per unsafe symlink actually dereferenced, at the requested info level only.
- [ ] **EC-7 — Match verbosity-to-debug mapping.** Resolve
  `verbose_1_no_debug_unlike_level_2` against upstream output. Level 1 must not
  enable debug categories; level 2 must enable the exact expected categories,
  including receiver diagnostics, without leaking debug output at lower levels.
- [ ] **EC-8 — Wire local-copy spill configuration.** Make
  `spill_env_e2e_engages_spill_layer` honor
  `OC_RSYNC_SPILL_THRESHOLD_BYTES` and `OC_RSYNC_SPILL_DIR` through
  `LocalCopyPlan::execute`. Validate threshold boundaries, disabled mode,
  cleanup, invalid values, and isolation between concurrent tests.

## Test-debt cleanup

- [ ] **EC-9 — Promote stale passing CLI tests.** Re-run
  `size_trailing_plus_minus_one_modifier_is_accepted` and
  `stderr_invalid_mode_is_rejected`, which again pass at this snapshot despite
  being ignored. Confirm they use the production parser, remove the stale
  ignores, and run them normally. Protocol-specific stale ignores are owned by
  the [protocol-version tracker](../protocol-versions/PLAN-TODO.md).

## Validation

- Run each named test alone with `cargo nextest run --run-ignored only` before
  its fix, then normally after removing its ignore.
- Add paired positive and negative cases for every selection rule so a fix does
  not merely invert the bug.
- Run `cargo nextest run` after all focused tests pass.

## Definition of done

Every named failing test is enabled and passing, the two stale CLI ignores are
removed, and no compatibility fix is limited to only one of two production
execution paths that share the option.

Security-sensitive path and filter cases belong in the
[hardening trackers](../hardening/PATH-CONFINEMENT.md).
