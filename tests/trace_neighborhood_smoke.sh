#!/usr/bin/env bash
# Linux integration: loopback-only ICMP trace + passive local neighborhood.
set -uo pipefail
binary="./target/release/pipi"
if [[ $# -gt 0 ]]; then binary="$1"; fi
failures=0
echo "Testing pipi trace / neighborhood (Linux, no external probes)"
help=$("$binary" trace --help 2>&1)
if [[ "$?" -eq 0 && "$help" == *"--smoke"* ]]; then
  echo "PASS trace help"; else echo "FAIL trace help"; failures=$((failures+1)); fi
graph=$("$binary" neighborhood --ascii --max-results 32 2>&1)
if [[ "$?" -eq 0 && "$graph" == *"LOCAL HOST"* && "$graph" == *"Observed"* ]]; then
  echo "PASS passive graphical neighborhood"; else echo "FAIL passive neighborhood"; echo "$graph"; failures=$((failures+1)); fi
# Probe strictly the local kernel loopback; never a router or public network.
output=$("$binary" trace -4 -c 2 --max-hops 1 -W 0.5 --smoke --ascii 127.0.0.1 2>&1)
trace_rc=$?
if [[ "$trace_rc" -eq 0 && "$output" == *"pipi trace 127.0.0.1"* && "$output" == *"cycle 2"* ]]; then
  echo "PASS IPv4 MTR-like trace and per-hop Smoke"
elif [[ "$output" == *"cannot open Linux ICMP ping socket"* || "$output" == *"IP_RECVERR required"* ]]; then
  echo "SKIP loopback trace: kernel rejects required ping socket or IP_RECVERR"
  if [[ "$failures" -eq 0 ]]; then exit 77; fi
else
  echo "FAIL IPv4 trace (exit $trace_rc)"; echo "$output"; failures=$((failures+1))
fi
if command -v script >/dev/null 2>&1; then
  capture=$(script -q -e -c "$binary trace -4 -c 1 --max-hops 1 -W 0.5 --tui 127.0.0.1" /dev/null 2>&1)
  if [[ "$?" -eq 0 && "$capture" == *"TRACE"* && "$capture" == *"pipi trace"* ]]; then
    echo "PASS compact trace TUI and terminal restore"
  else
    echo "FAIL trace TUI"; failures=$((failures+1))
  fi
  capture=$(script -q -e -c "$binary neighborhood --tui -c 1 --ascii" /dev/null 2>&1)
  if [[ "$?" -eq 0 && "$capture" == *"NEIGHBORHOOD"* && "$capture" == *"LOCAL HOST"* ]]; then
    echo "PASS graphical neighborhood TUI and terminal restore"
  else
    echo "FAIL neighborhood TUI"; failures=$((failures+1))
  fi
else
  echo "SKIP pseudo-TTY tests: script(1) unavailable"
fi
echo "Trace+neighborhood failures=$failures"
if [[ "$failures" -gt 0 ]]; then exit 1; fi
exit 0
