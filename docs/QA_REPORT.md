# QA report – implementation candidate

Status is deliberately grounded in real test execution, not source presence.

- Development branch: `feat/rust-linux-ping-engine`.
- PR: https://github.com/derdigitalaffine/piping/pull/18 (draft pending acceptance).
- Proposed 0.1.0 scope: std-only CLI, Linux datagram ping sockets IPv4/IPv6, bounded scheduler, Fast Ping, CIDR/IP-range sweep, online stats, Smoke-style terminal chart, optional full-screen UI feature, JSONL/CSV.
- The assistant execution container does **not** include `rustc`, `cargo` or `rustfmt`, and cannot fetch the toolchain from the internet.
- Actual Linux CI run #41803320285 successfully compiled the Rust crate and ran **21 unit tests + 4 CLI tests**, but the following formatting check failed (source later reformatted). Other gates still require a verified all-green workflow.
- No claim of production release, measured PPS advantage, full kernel-platform compatibility, or accepted CI before verification.

## Mandatory gate commands

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo build --release --locked --no-default-features
cargo build --release --locked --all-features
bash tests/linux_smoke.sh ./target/release/piping
```

The last command may exit 77 for explicitly skipped ICMP sockets (permission / missing IPv6), which is **not** a successful IPv6/privilege verification.

## Required additional checks

1. Restrict Linux `ping_group_range` in an isolated namespace and verify no privilege escalation and actionable EPERM text.
2. Validate full IPv4 and IPv6 packet exchange, duplicate and reordered packets in a controlled test network.
3. Run `piping -c 3 127.0.0.1`, fast/range/monitor and machine-output golden tests on Linux.
4. Verify rate, outstanding, payload, host caps, Ctrl-C and broken-pipe handling.
5. Build from clean checkout on x86_64 and ideally aarch64; record distro/kernel/MSRV.
6. Benchmarks only on loopback / explicitly authorized test networks.

An issue is not closed merely because code exists. Record CI job IDs and actual results before closing P0 issues.
