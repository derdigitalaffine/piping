#!/usr/bin/env bash
# Controlled, short IPv4 loopback measurement. Never flood external destinations.
set -uo pipefail
binary="./target/release/pipi"
if [[ $# -ge 1 ]]; then binary="$1"; fi
count=201
pps=100
ms_min=1800
out=$(mktemp)
metrics=$(mktemp)
cleanup() { rm -f "$out" "$metrics"; }
trap cleanup EXIT

echo "Benchmark environment: $(uname -srm), binary=$binary, target=127.0.0.1 count=$count pps=$pps"
start=$(date +%s%3N)
if [[ -x /usr/bin/time ]]; then
  /usr/bin/time -f 'wall_s=%e cpu_pct=%P max_rss_kb=%M' -o "$metrics" \
    "$binary" fast -4 -q -c "$count" --pps "$pps" -W 1 127.0.0.1 > "$out" 2>&1
  rc=$?
else
  "$binary" fast -4 -q -c "$count" --pps "$pps" -W 1 127.0.0.1 > "$out" 2>&1
  rc=$?
fi
end=$(date +%s%3N)
elapsed=$((end - start))
if [[ "$rc" -ne 0 ]]; then
  if grep -qiE 'ping socket not permitted|requested IP family is unavailable' "$out"; then
    echo "SKIP: no ICMP ping socket on benchmark host"
    exit 77
  fi
  echo "FAIL: benchmark process exited $rc"
  cat "$out"
  exit 1
fi
if ! grep -Fq "$count packets transmitted, $count received" "$out"; then
  echo "FAIL: incomplete loopback replies"
  cat "$out"
  exit 1
fi
if [[ "$elapsed" -lt "$ms_min" ]]; then
  echo "FAIL: elapsed=$elapsed ms is too short for $count probes at global cap $pps PPS"
  cat "$out"
  exit 1
fi
echo "PASS: $count/$count loopback probes at <= $pps PPS; elapsed=$elapsed ms"
cat "$metrics"
tail -n 2 "$out"
