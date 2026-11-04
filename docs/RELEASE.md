# Release engineering (0.1.0)

## Preconditions

Linux CI, Rust 1.85 builds and live IPv4/IPv6 loopback checks are passing as of run [#41805747384](https://github.com/derdigitalaffine/piping/actions/runs/41805747384). Release only after remaining QA restrictions and unsafe FFI review are cleared, an independent Linux installation has been checked, and the decision is recorded in `docs/QA_REPORT.md`.

## Source build

```sh
cargo build --release --locked
./target/release/piping --help
./target/release/piping -4 -c 3 127.0.0.1
```

Linux rootless installation, when the kernel allows ping datagram sockets:

```sh
cargo install --path . --locked
piping -c 3 127.0.0.1
```

This project has no crates.io third-party dependencies. It uses a small audited unsafe POSIX socket wrapper (see `src/linux.rs`), which still needs platform-specific review and actual runtime tests.

## Optional fullscreen UI

```sh
cargo build --release --locked --features tui
./target/release/piping tui ::1
```

## Packaging

CI currently produces local x86_64 minimal and TUI tarballs plus SHA256SUMS. GitHub Actions storage quota was exhausted when an upload was attempted, so publishing is deliberately optional on version tags only. No artifact is currently claimed as downloadable. Do not publish an unverified binary; before public distribution, test aarch64, record checksums, choose a license, create a verified version tag and generate release notes.

No install step automatically runs `sudo`, `setcap`, or mutates kernel settings. If datagram ping sockets are disallowed, document the operator's group-policy prerequisite; do not silently relax system security.
