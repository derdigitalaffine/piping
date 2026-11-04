#!/usr/bin/env bash
# Disposable virtual-only two-host ARP test. Never probe host/public interfaces.
set -uo pipefail
binary="./target/release/pipi"
if [[ $# -gt 0 ]]; then binary="$1"; fi
if ! command -v ip >/dev/null 2>&1 || ! command -v sudo >/dev/null 2>&1 || ! sudo -n true 2>/dev/null; then
  echo "SKIP: cannot create isolated veth pair"; exit 77
fi
iface="pipi-ci0"
peer="pipi-ci1"
cleanup() { sudo -n ip link del "$iface" 2>/dev/null || true; }
if ip link show "$iface" >/dev/null 2>&1 || ip link show "$peer" >/dev/null 2>&1; then
  echo "SKIP: reserved test interface name already exists; do not modify it"
  exit 77
fi
if ! sudo -n ip link add "$iface" type veth peer name "$peer" 2>/dev/null; then
  echo "SKIP: kernel/runner denies creating veth"; exit 77
fi
trap cleanup EXIT
if ! sudo -n ip addr add 198.18.0.1/30 dev "$iface" \
  || ! sudo -n ip addr add 198.18.0.2/30 dev "$peer" \
  || ! sudo -n ip link set "$iface" up \
  || ! sudo -n ip link set "$peer" up; then
  echo "SKIP: failed to configure isolated veth"; exit 77
fi
# The binary itself performs no privilege escalation; sudo is test harness only.
output=$(sudo -n "$binary" discover --active 198.18.0.0/30 \
  --interface "$iface" --pps 5 --max-hosts 2 --timeout 2 --output jsonl 2>&1)
status=$?
if [ "$status" -ne 0 ]; then
  if grep -qi 'CAP_NET_RAW required' <<< "$output"; then
    echo "SKIP: isolated runner lacks CAP_NET_RAW"; exit 77
  fi
  echo "FAIL: active ARP exit=$status"; echo "$output"; exit 1
fi
if ! python3 -c '
import json, sys
data = [json.loads(line) for line in sys.stdin if line.strip()]
hosts = [r for r in data if r["event"] == "neighbor"]
assert len(hosts) == 1, data
assert hosts[0]["ip"] == "198.18.0.2", data
assert hosts[0]["source"] == "active-arp", data
assert hosts[0]["mac"].count(":") == 5, data
' <<< "$output"; then
  echo "FAIL: invalid active ARP result"; echo "$output"; exit 1
fi
# Off-link network must fail before any ARP send.
sudo -n "$binary" discover --active 198.18.0.0/29 \
  --interface "$iface" --pps 5 --max-hosts 6 >/dev/null 2>&1
offlink_status=$?
if [ "$offlink_status" -ne 2 ]; then
  echo "FAIL: off-link range was not rejected (exit=$offlink_status)"; exit 1
fi
echo "PASS: explicit active ARP discovered one veth peer, and rejected off-link sweep"
