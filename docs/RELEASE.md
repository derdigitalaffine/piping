# Release engineering (0.1.0)

## Preconditions

Release only after Linux CI and real loopback ICMP checks pass, tests and documentation agree, and a release decision is recorded in `docs/QA_REPORT.md`.

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

Do not publish an unverified binary. Once accepted, produce and test reproducible Linux x86_64 and aarch64 binaries, SHA256 manifest, `CHANGELOG.md`, signed source tag, and explicit license/redistribution decision by repository owner. No pre-generated release binaries are claimed.

No install step automatically runs `sudo`, `setcap`, or mutates kernel settings. If datagram ping sockets are disallowed, document the operator's group-policy prerequisite; do not silently relax system security.
