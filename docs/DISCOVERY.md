# Linux Layer-2 Discovery – ARP/NDP

Development scope: [#21](https://github.com/derdigitalaffine/piping/issues/21) (passive), [#22](https://github.com/derdigitalaffine/piping/issues/22) (active). Part of the next source candidate, not an announced binary release.

## Passive (no privileged socket or scan)

```sh
piping discover
piping discover --interface eth0
piping discover --family 6 --output jsonl
piping discover --output csv --max-results 100
```

The default reads the kernel neighbor table using `NETLINK_ROUTE / RTM_GETNEIGH`. IPv4 is the ARP neighbor cache; IPv6 is the NDP cache. Results may be STALE, FAILED or incomplete, or entirely empty. **This is not a network sweep**, and absence from the cache says nothing about host availability. Interface names, IP addresses, MAC/link addresses and neighbor states come from the local Linux kernel, not DNS or an external API. No probes are sent and the program does not ask for root.

The hard cap is 8192 parsed neighbors; at most 4096 are printed by default, configurable with `--max-results 1..4096`. When rows are truncated, the text and JSONL output report the condition. CSV has only rows and a fixed header.

## Active – explicit and local IPv4 ARP

```sh
piping discover --active 192.168.1.0/24 --interface eth0
piping discover --active 192.168.1.0/24 -i eth0 --pps 20 --max-hosts 256 --timeout 2 --output jsonl
```

Active discovery requires an explicit IPv4 CIDR **and** an existing named Linux Ethernet interface. It first checks that **every** target is on a directly connected, gateway-free route for that interface (`/proc/net/route`), excluding the interface's own IPv4 address. IPv4 network/broadcast addresses are omitted for prefixes up to /30; /31 includes both, /32 its one address. No inferred subnet, automatic expansion, default scan or external target.

Bounded at 256 target addresses, 100 ARP frames per second, and a 0.1–5 second response wait after the final send (2s default). Default ARP send cap is 20 PPS. All frames are matched to the chosen interface and solicited target IP; replies must carry the expected destination IPv4 and local MAC and consistent Ethernet/ARP source MAC. Results are deduplicated per IP.

For active mode the Linux kernel requires `CAP_NET_RAW` (or an appropriate administrator-managed privileged execution). If access is denied, the CLI explains the missing permission. **piping does not run sudo, enable capabilities, adjust sysctl or probe without an explicit opt-in.** Avoid globally granting raw-socket privileges to all binaries. Do not scan networks unless authorized.

## Output and exit codes

- Text: IP address, Linux interface, MAC, NUD state. Summary includes rows shown and truncation.
- JSONL: `schema:1` neighbor events with `ip`, `interface`, `mac`, `state`, `source`, plus a summary event (`shown`, `total`, `truncated`).
- CSV: `ip,interface,mac,state,source`.
- Exit `0`: completed discovery, including an empty neighbor table; `2`: invalid options, unsupported privileges, transport/route errors. Unlike ICMP ping, finding 0 L2 hosts is not an exit-1 error.

## Security / limitations

ARP replies are **not cryptographically authenticated**; an on-link attacker can spoof identities. Passive NDP cache entries may not represent reachable hosts. IPv6 NDP active solicitation is **not** in this phase; IPv6 is read-only via rtnetlink. Neither mode scans TCP/UDP ports. Layer-2 discovery cannot cross routers by design. Scans against public hosts are not used in tests.

Implementation: `src/neighbor.rs` (safe rtnetlink parsing), `src/arp.rs` (explicit packet socket), `src/discovery.rs` (validation/output). No extra Cargo crates. Tests include malicious/truncated Netlink/ARP fixtures, passive live Linux reading, and optional **isolated** veth-peer ARP with explicit SKIP if Linux CI lacks network-admin capability. The new branch must pass all gates before merge.
