# piping CLI reference (0.1.0 candidate)

Linux-only ICMP Echo diagnostics in Rust. The default mode behaves like a conventional one-host ping; no TUI or external Rust dependencies are required.

| Command | Meaning |
| --- | --- |
| `piping HOST` | classic continuous Echo until Ctrl-C |
| `piping -4 -c 4 -W 2 HOST` | four IPv4 probes with two-second timeout |
| `piping -6 ::1` | force IPv6 |
| `piping fast 127.0.0.1 --pps 20 --count 100` | bounded high-frequency Echo |
| `piping range 192.168.1.0/24 --only-alive` | IPv4 CIDR ICMP sweep |
| `piping range 192.168.1.10-192.168.1.20 --jobs 16` | inclusive IPv4 range sweep |
| `piping range 2001:db8::/126 --max-hosts 16` | bounded IPv6 CIDR sweep |
| `piping monitor localhost --window 60 --ascii` | streaming text RTT distribution |
| `piping tui localhost` | opt-in fullscreen terminal (requires `--features tui`) |

## Options

- `-c, --count N`: sent probes. Classic/monitor infinite by default; fast default 100; range default 1 **per host**.
- `-i, --interval SEC`: interval for classic/monitor/tui; default 1s classic, 0.25s monitor. Between 0.02s and 3600s.
- `-W, --timeout SEC`: individual probe timeout (default 2s).
- `-4` / `-6`: force IP family; otherwise IPv4 preferred for dual-stack hostnames.
- `-n`: use numeric address in classic header; reply addresses are always numeric.
- `-q`: suppress per-probe events/graph but preserve summary.
- `--ttl N`: outbound IPv4 TTL or IPv6 Hop Limit, 1..255.
- `--size N`: ICMP data bytes, 16..1400; default 56 (16 bytes reserved for unique probe correlation).
- `--pps N`: global send rate cap in fast/range, 1..1000; default 20.
- `--jobs N`: maximum pending probes in fast/range, 1..128; defaults 64 fast / 32 range.
- `--max-hosts N`: range cap 1..4096, default 256.
- `--only-alive`, `--require-all`: range output and return-code policy.
- `--window N`: graph history (5..3600 seconds).
- `--ascii`, `--no-color`, `--output text|jsonl|csv`: terminal and machine output.

## Exit codes

`0` = a valid Echo Reply (or all hosts if `--require-all` is set in range); `1` = zero replies or the required hosts did not all respond; `2` = CLI/DNS/socket/runtime error. Ctrl-C prints the summary and exits with the corresponding result code.

## CIDR behavior

IPv4 CIDR `/0..=/30` excludes network and broadcast; `/31` includes both endpoints and `/32` one address. IPv4 `START-END` is inclusive. IPv6 CIDR includes all addresses but is strictly capped; a prefix producing more than 4096 addresses is rejected. No silent truncation, port scanning, or implicit neighbor discovery.

## Smoke graph and optional terminal UI

A bucket groups probes sent in a one-second window. The graph shows median as `*` (ASCII) or `●` (Unicode) and p10–p90 spread as `:` or shaded `▒`. p95, timed-out percentage and still-pending probes are printed distinctly. Pending requests **do not** count as loss before deadline. ASCII is selected automatically in pipes or via `--ascii`. The optional `tui` feature uses an alternate-screen panel; Ctrl-C exits and restores the terminal. Text ping and monitor work without the feature.

## Machine-readable output

`--output jsonl` emits `schema:1` reply, timeout and per-target summary objects with UTC epoch milliseconds in `timestamp_unix_ms`. `--output csv` uses columns `event,target,ip,seq,rtt_ms,sent,received,loss_pct,timestamp_unix_ms`. String fields are escaped, and neither format uses ANSI sequences. The program flushes output incrementally and treats closed pipes as normal termination.

## Limitations

- Deliberate subset of iputils ping, not a drop-in implementation of its full options/error formats.
- Linux SOCK_DGRAM ICMP sockets only; the kernel manages Echo ID and validates checksums. No automatic privileged Raw-Socket fallback.
- The **outbound** TTL/hop limit is configurable; received-packet TTL/hop limit is not displayed because ancillary data retrieval is not implemented.
- Hostname resolution uses the system resolver under a five-second timeout (numeric IPv4/IPv6 addresses bypass DNS). This deadline is separate from the per-probe ICMP timeout. A stalled NSS thread may remain until process exit.
- Percentiles for long-running sessions summarize a bounded rolling sample of the latest 2048 RTT values; min/avg/max/mdev cover all accepted replies.
- The lightweight optional TUI is an alternate-screen presentation with Ctrl-C exit, not a full keybinding-driven curses application.
