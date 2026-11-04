#!/usr/bin/env bash
# Optional *actual* loopback tests; no public destinations, no privileges changed.
set -uo pipefail
binary="./target/debug/piping"
if [[ $# -ge 1 ]]; then binary="$1"; fi
failed=0
skipped=0

run() {
  local label="$1"
  shift
  local output status
  output=$("$binary" "$@" 2>&1)
  status=$?
  if [[ "$status" -eq 0 ]]; then
    echo "PASS $label"
    echo "$output" | tail -n 3
  elif grep -qiE 'ping socket not permitted|requested IP family is unavailable|address family not supported' <<< "$output"; then
    echo "SKIP $label: Linux kernel/namespace lacks ICMP ping socket for this family"
    echo "$output" | tail -n 2
    skipped=$((skipped + 1))
  else
    echo "FAIL $label (exit=$status)"
    echo "$output"
    failed=$((failed + 1))
  fi
}

run "IPv4 classic" -4 -c 3 -W 1 127.0.0.1
run "IPv6 classic" -6 -c 3 -W 1 ::1
run "Fast IPv4" fast -4 -c 5 --pps 20 -W 1 127.0.0.1
run "Range IPv4" range -4 -c 1 --max-hosts 1 -W 1 127.0.0.1/32
run "Smoke text" monitor -4 -c 5 -i 0.25 -W 1 --ascii 127.0.0.1

echo "Loopback smoke: failed=$failed skipped=$skipped"
if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
if [[ "$skipped" -gt 0 ]]; then
  exit 77
fi
exit 0
