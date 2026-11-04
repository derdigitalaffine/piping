# Changelog

## [Unreleased] – pipi v0.2.0 development candidate (2026-11-04)

- **Breaking CLI rename:** Cargo crate/binary `pipi` instead of `piping`; same GitHub repository `derdigitalaffine/piping`.
- `pipi trace HOST`: MTR-like ICMPv4/v6 hop statistics from Linux MSG_ERRQUEUE with per-hop bounded SmokePing-style loss/RTT sparklines and optional compact TUI.
- `pipi neighborhood`: per-interface graphical ARP/NDP neighbor graph, read-only 5s auto-refresh and optional `--scan-local` explicit private on-link ARP probing.
- Redesigned viewport-limited fullscreen TUI for classic ping, trace and neighborhood; min/avg/max/p95/jitter in ping display.
- **Pending**: Rust build, clippy/fmt, live Linux validation and unsafe audit; Actions jobs have failed before startup (issue #24).

## [Previous development candidate] – piping v0.1.0 (2026-11-04)

- Dependency-free Rust 2024 Linux binary with conventional one-host ICMP ping.
- Linux datagram ICMPv4 / ICMPv6 Echo transport, probe source/sequence/nonce correlation, timeout and duplicate accounting.
- Bounded `fast` mode with configured global packets-per-second cap and outstanding-probe ceiling.
- Explicit `range` CIDR and inclusive IPv4 endpoint sweep, target caps and alive/down summary.
- Smoke-style rolling RTT median, spread/p95, counted losses and pending probes as ASCII/Unicode console output.
- Optional Fullscreen TUI with alternate-screen restoration; classic and monitor remain TUI-independent.
- Streaming JSONL/CSV outputs with UTC epoch-millisecond timestamps, script-friendly exit codes, signal termination and broken-pipe handling.
- Five-second upper bound on hostname resolution and a fast numeric-IP path.
- CI source installation with isolated build target and installed-binary loopback smoke.
- Linux CI with stable+Rust1.85 tests, fmt, clippy, two release builds, actual IPv4/IPv6 loopback, protocol fixtures and bounded performance benchmark.

Pre-release candidate; see [QA report](docs/QA_REPORT.md) for passed tests and outstanding release gates.
