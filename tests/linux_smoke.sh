#!/usr/bin/env bash
# Real Linux loopback acceptance, without external network traffic or sysctl edits.
set -uo pipefail
binary="./target/debug/piping"
if [[ $# -ge 1 ]]; then binary="$1"; fi
failed=0
skipped=0

pass() { echo "PASS $1"; }
fail() { echo "FAIL $1"; failed=$((failed + 1)); }

run() {
  local label="$1" expected="$2"
  shift 2
  local output status
  output=$("$binary" "$@" 2>&1)
  status=$?
  if [[ "$status" -eq 0 ]] && grep -Fq "$expected" <<< "$output"; then
    pass "$label"
    echo "$output" | tail -n 3
  elif grep -qiE 'ping socket not permitted|requested IP family is unavailable|address family not supported' <<< "$output"; then
    echo "SKIP $label: kernel/namespace denies ICMP ping sockets"
    echo "$output" | tail -n 2
    skipped=$((skipped + 1))
  else
    fail "$label (exit=$status, expected: $expected)"
    echo "$output"
  fi
}

run "IPv4 classic" "3 packets transmitted, 3 received" -4 -c 3 -W 1 127.0.0.1
run "IPv6 classic" "3 packets transmitted, 3 received" -6 -c 3 -W 1 ::1
run "Fast IPv4" "5 packets transmitted, 5 received" fast -4 -c 5 --pps 20 -W 1 127.0.0.1
run "Range IPv4" "1 scanned, 1 alive" range -4 -c 1 --max-hosts 1 -W 1 127.0.0.1/32
run "Smoke text graph" "t+0000s" monitor -4 -c 9 -i 0.25 -W 1 --ascii 127.0.0.1

# Negative/fallback conditions, machine formats and signals need an available ping socket.
if [[ "$skipped" -eq 0 ]] && [[ "$failed" -eq 0 ]]; then
  json_output=$("$binary" -4 -c 2 -W 1 --output jsonl 127.0.0.1 2>&1)
  json_status=$?
  if [[ "$json_status" -eq 0 ]] && python3 -c '
import json, sys
lines = [json.loads(line) for line in sys.stdin if line.strip()]
assert sum(e.get("event") == "reply" for e in lines) == 2, lines
assert sum(e.get("event") == "summary" for e in lines) == 1, lines
assert all(e.get("schema") == 1 for e in lines), lines
assert all(e.get("received") == 2 for e in lines if e["event"] == "summary"), lines
' <<< "$json_output"; then
    pass "JSONL schema, 2 replies and final summary"
  else
    fail "JSONL schema / event counts"
    echo "$json_output"
  fi

  csv_output=$("$binary" -4 -c 2 -W 1 --output csv 127.0.0.1 2>&1)
  csv_status=$?
  if [[ "$csv_status" -eq 0 ]] && python3 -c '
import csv, sys
rows = list(csv.DictReader(sys.stdin))
assert len(rows) == 3, rows
assert [r["event"] for r in rows] == ["reply", "reply", "summary"], rows
assert rows[-1]["received"] == "2", rows
' <<< "$csv_output"; then
    pass "CSV schema, two replies and summary"
  else
    fail "CSV schema / event counts"
    echo "$csv_output"
  fi

  # The process receives SIGINT, prints a summary, and terminates normally.
  tmp=$(mktemp)
  "$binary" -4 -W 1 127.0.0.1 > "$tmp" 2>&1 &
  child=$!
  sleep 0.5
  kill -INT "$child" 2>/dev/null
  wait "$child"
  status=$?
  if [[ "$status" -eq 0 ]] && grep -q "piping statistics" "$tmp"; then
    pass "SIGINT prints statistics and exits 0"
  else
    fail "SIGINT handling (exit=$status)"
    cat "$tmp"
  fi
  rm -f "$tmp"

  # A consumer closing the pipe must not panic the sender.
  "$binary" fast -4 -c 50 --pps 100 -W 1 127.0.0.1 | head -n 2 > /dev/null
  status=$?
  if [[ "$status" -eq 0 ]]; then
    pass "Closed stdout pipe (no EPIPE panic)"
  else
    fail "Closed stdout pipe (exit=$status)"
  fi

  if command -v script >/dev/null 2>&1; then
    tui_pty_output=$(script -q -e -c "$binary tui -4 -c 6 -i 0.25 127.0.0.1" /dev/null 2>&1)
    tui_pty_status=$?
    if [[ "$tui_pty_status" -eq 0 ]] && [[ "$tui_pty_output" == *
  tui_status=$?
  if [[ "$tui_status" -eq 2 ]] && grep -q "needs an interactive terminal" <<< "$tui_output"; then
    pass "Optional TUI rejects non-interactive stdout"
  else
    fail "TUI non-tty guard (exit=$tui_status)"
    echo "$tui_output"
  fi
else
  echo "SKIP advanced runtime checks because ping sockets were unavailable or basic tests failed"
  skipped=$((skipped + 5))
fi

echo "Linux loopback acceptance: failed=$failed skipped=$skipped"
if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
if [[ "$skipped" -ne 0 ]]; then
  exit 77
fi
exit 0
\e[?1049h'* ]] && [[ "$tui_pty_output" == *
  tui_status=$?
  if [[ "$tui_status" -eq 2 ]] && grep -q "needs an interactive terminal" <<< "$tui_output"; then
    pass "Optional TUI rejects non-interactive stdout"
  else
    fail "TUI non-tty guard (exit=$tui_status)"
    echo "$tui_output"
  fi
else
  echo "SKIP advanced runtime checks because ping sockets were unavailable or basic tests failed"
  skipped=$((skipped + 5))
fi

echo "Linux loopback acceptance: failed=$failed skipped=$skipped"
if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
if [[ "$skipped" -ne 0 ]]; then
  exit 77
fi
exit 0
\e[?1049l'* ]]; then
      pass "Optional TUI enters and restores alternate screen in a real pseudo-terminal"
    else
      fail "Optional TUI pseudo-terminal rendering/restore (exit=$tui_pty_status)"
      echo "$tui_pty_output" | tail -c 200
    fi
  else
    echo "SKIP optional TUI pseudo-terminal check: util-linux 'script' absent"
    skipped=$((skipped + 1))
  fi

  tui_output=$("$binary" tui -4 -c 1 127.0.0.1 2>&1)
  tui_status=$?
  if [[ "$tui_status" -eq 2 ]] && grep -q "needs an interactive terminal" <<< "$tui_output"; then
    pass "Optional TUI rejects non-interactive stdout"
  else
    fail "TUI non-tty guard (exit=$tui_status)"
    echo "$tui_output"
  fi
else
  echo "SKIP advanced runtime checks because ping sockets were unavailable or basic tests failed"
  skipped=$((skipped + 5))
fi

echo "Linux loopback acceptance: failed=$failed skipped=$skipped"
if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
if [[ "$skipped" -ne 0 ]]; then
  exit 77
fi
exit 0
