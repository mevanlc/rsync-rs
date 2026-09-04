# TCP Hardening

Implementation snapshot: 2026-09-18, `ac33a61d4`, compared directly with the
original 2026-09-04 baseline `23e562215`.

This tracker owns behavior that is reachable specifically through the rsync
daemon's listening-socket transport: connection establishment, handshake
deadlines, PROXY protocol trust, and bounded proxy request/response framing.
The generated [Linux non-root](../../../tools/ci/upstream-3.5.0-expect.nonroot.tcp.txt),
[Linux root](../../../tools/ci/upstream-3.5.0-expect.root.tcp.txt),
[macOS non-root](../../../tools/ci/upstream-3.5.0-expect.macos.nonroot.tcp.txt),
and [macOS root](../../../tools/ci/upstream-3.5.0-expect.macos.root.tcp.txt) TCP
manifests are the source of truth.

## Completion contract

- Make trust decisions from the actual connected peer before consuming or
  honoring claimed source-address metadata.
- Fail closed when PROXY protocol is enabled without a matching trusted-peer
  rule.
- Bound bytes, allocation, and elapsed time before a session is authenticated.
- Return an upstream-compatible failure and close the connection without
  starting a transfer or leaving per-connection state behind.

All five TCP-owned rows now pass in the Linux and macOS TCP manifests.

## P0: Trust and authentication boundary

- [x] **TCP-1 — Implement `proxy protocol hosts`.**
  `daemon-proxy-protocol` and `proxy-protocol-trusted-peer` pass in all four TCP
  manifests. The trusted direct-peer list is evaluated before consuming the
  header, and absent or empty trust configuration fails closed.
- [x] **TCP-2 — Enforce the unauthenticated handshake deadline.**
  `daemon-handshake-timeout` passes in all four TCP manifests. One absolute
  monotonic deadline bounds the greeting, capability, module, and authentication
  prelude rather than resetting on trickled bytes.

## P0: Bounded framing

- [x] **TCP-3 — Bound proxy request and response headers.**
  `proxy-connect-request-too-long` and `proxy-response-header-too-long` pass in
  all four TCP manifests. Request, authorization, status, and response header
  limits are enforced before an over-limit line is written or retained.

## Validation

- Use loopback sockets with deterministic deadlines; do not rely on public
  network services.
- Run the named tests in both non-root and root TCP corpora and regenerate both
  manifests only after they pass against an upstream rsync 3.5.0 negative
  control.
- Add concurrency tests proving that slow or malformed unauthenticated clients
  cannot exhaust connection permits indefinitely.
- Run `cargo nextest run -E 'deps(daemon)'`, then the full suite.

## Definition of done

Satisfied at this snapshot: all five TCP-owned rows pass in both privilege legs
on Linux and macOS, `proxy protocol hosts` fails closed, and the byte and time
bounds are regression-tested.

Related trackers: [daemon](DAEMON.md),
[path confinement](PATH-CONFINEMENT.md), and
[miscellaneous security](MISC-SECURITY.md).
