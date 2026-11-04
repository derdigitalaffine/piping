#!/usr/bin/env bash
# High-rate measurement ONLY against the local kernel loopback, no external target.
set -uo pipefail
binary="./target/release/piping"
if [[ $# -gt 0 ]]; then binary="$1"; fi
out=$(mktemp)
metrics=$(mktemp)
trap 'rm -f "$out" "$metrics"' EXIT
echo "Fastpath: $(uname -srm), 1001 probes at <=1000 PPS, IPv4 loopback"
begin=$(date +%s%3N)
if [[ -x /usr/bin/time ]]; then
  /usr/bin/time -f 'wall_s=%e user_s=%U sys_s=%S cpu_pct=%P max_rss_kb=%M' -o "$metrics" \
    "$binary" fast --pps 1000 -4 -c 1001 -W 1 -q 127.0.0.1 > "$out" 2>&1
  code=$?
else
  "$binary" fast --pps 1000 -4 -c 1001 -W 1 -q 127.0.0.1 > "$out" 2>&1
  code=$?
fi
end=$(date +%s%3N)
if [[ "$code" -ne 0 ]]; then
  if grep -qi 'ping socket not permitted' "$out"; then
    echo "SKIP: ping sockets denied by Linux"; exit 77
  fi
  echo "FAIL: benchmark process returned $code"; cat "$out"; exit 1
fi
if ! grep -q '1001 packets transmitted, 1001 received' "$out"; then
  echo "FAIL: 1001/1001 not received"; cat "$out"; exit 1
fi
elapsed=$((end - begin))
if [[ "$elapsed" -lt 950 ]]; then
  echo "FAIL: rate bound exceeded (elapsed=$elapsed ms)"; cat "$out"; exit 1
fi
echo "PASS fastpath: 1001/1001 echo, elapsed=$elapsed ms"
cat "$metrics"
tail -n 2 "$out"
