#!/usr/bin/env bash
# Same-host controlled comparison. Both binaries run against 127.0.0.1 only.
# Never claim "faster" without the actual recorded CPU/wall/RSS numbers.
set -uo pipefail
if [[ $# -ne 2 ]]; then
  echo "USAGE: compare_fastpath.sh BASELINE_BINARY NEW_BINARY" >&2
  exit 2
fi
baseline="$1"
candidate="$2"
if [[ ! -x "$baseline" || ! -x "$candidate" ]]; then
  echo "Both Linux binaries must exist and be executable" >&2
  exit 2
fi
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Same-runner benchmark: $(uname -srm); 1001 packets at <=1000 PPS"
for mode in baseline candidate; do
  if [[ "$mode" == baseline ]]; then binary="$baseline"; else binary="$candidate"; fi
  for trial in 1 2 3; do
    start=$(date +%s%3N)
    if [[ -x /usr/bin/time ]]; then
      /usr/bin/time -f 'user_sec=%U sys_sec=%S cpu_pct=%P max_rss_kb=%M' \
        -o "$tmp/metrics" "$binary" fast -4 -c 1001 --pps 1000 -W 1 -q 127.0.0.1 \
        > "$tmp/out" 2>&1
      rc=$?
    else
      "$binary" fast -4 -c 1001 --pps 1000 -W 1 -q 127.0.0.1 > "$tmp/out" 2>&1
      rc=$?
    fi
    end=$(date +%s%3N)
    if [[ "$rc" -ne 0 ]] || ! grep -q '1001 packets transmitted, 1001 received' "$tmp/out"; then
      if grep -qi 'ping socket not permitted' "$tmp/out"; then
        echo "SKIP: ICMP sockets are disabled on this runner"; exit 77
      fi
      echo "FAIL $mode trial $trial: status=$rc"; cat "$tmp/out"; exit 1
    fi
    echo "$mode run=$trial wall_ms=$((end - start)) $(cat "$tmp/metrics")"
  done
done
echo "Measured results only; compare wall/CPU/RSS on identical kernel and host."
