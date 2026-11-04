#!/usr/bin/env bash
# Real passive Linux neighbor discovery: read-only rtnetlink, no root or probes.
set -euo pipefail
binary="./target/release/pipi"
if [[ $# -gt 0 ]]; then binary="$1"; fi
"$binary" discover --help | grep -q 'RTM_GETNEIGH'
json=$(mktemp)
csv=$(mktemp)
trap 'rm -f "$json" "$csv"' EXIT
"$binary" discover --output jsonl --max-results 64 > "$json"
python3 - "$json" <<'PY'
import json, sys, ipaddress
records = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8") if line.strip()]
assert records and records[-1]["event"] == "summary", records
assert all(item["schema"] == 1 for item in records), records
for row in records[:-1]:
    assert row["event"] == "neighbor", row
    ipaddress.ip_address(row["ip"])
    assert row["source"] == "kernel-neighbor-cache", row
assert records[-1]["shown"] == len(records) - 1, records
assert records[-1]["total"] >= records[-1]["shown"]
print("PASS passive rtnetlink JSONL (%d neighbor(s))" % records[-1]["shown"])
PY
"$binary" discover --output csv --family 6 --max-results 64 > "$csv"
python3 - "$csv" <<'PY'
import csv, ipaddress, sys
with open(sys.argv[1], encoding="utf-8") as file:
    rows = list(csv.DictReader(file))
for row in rows:
    assert ipaddress.ip_address(row["ip"]).version == 6
    assert row["source"] == "kernel-neighbor-cache"
print("PASS passive IPv6 NDP CSV (%d neighbor(s))" % len(rows))
PY
set +e
"$binary" discover --active 198.18.0.0/30 --pps 101 --interface pipi-ci0 >/dev/null 2>&1
rc=$?
set -e
test "$rc" -eq 2
echo "PASS active ARP flag limits reject invalid rate without traffic"
