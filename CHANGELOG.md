# Changelog

## [Unreleased] – v0.1.0 candidate (2026-11-04)

- Dependency-free Rust 2024 Linux binary with conventional one-host ICMP ping.
- Linux datagram ICMPv4 / ICMPv6 Echo transport, probe source/sequence/nonce correlation, timeout and duplicate accounting.
- Bounded `fast` mode with configured global packets-per-second cap and outstanding-probe ceiling.
- Explicit `range` CIDR and inclusive IPv4 endpoint sweep, target caps and alive/down summary.
- Smoke-style rolling RTT median, spread/p95, counted losses and pending probes as ASCII/Unicode console output.
- Optional Fullscreen TUI with alternate-screen restoration; classic and monitor remain TUI-independent.
- Streaming JSONL/CSV outputs, script-friendly exit codes, signal termination and broken-pipe handling.
- Linux CI with stable+Rust1.85 tests, fmt, clippy, two release builds, actual IPv4/IPv6 loopback, protocol fixtures and bounded performance benchmark.

Pre-release candidate; see [QA report](docs/QA_REPORT.md) for passed tests and outstanding release gates.
