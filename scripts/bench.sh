#!/usr/bin/env bash
# Latency and weight of the web app's routes.
#
#   scripts/bench.sh [base-url] [requests-per-route] [todo-id]
#
# Prints p50 / p95 / max in milliseconds and the gzip wire size per route, against a running
# `suzatary serve` (default: the instance's `web.app_url`).
set -euo pipefail
instance="${SUZATARY_INSTANCE:-$(cd "$(dirname "$0")/.." && pwd)/instance.toml}"
url() {
  local bin="${SUZATARY_BIN:-$("$(dirname "$0")/app-bin.sh")/bin/suzatary}"
  "$bin" --instance "$instance" instance get web.app_url 2>/dev/null || true
}
base="${1:-$(url)}"
[ -n "$base" ] || { echo "bench: name a base URL, or an instance file with app_url" >&2; exit 2; }
n="${2:-20}"
routes=(
  "/api/chain"
  "/api/fold"
  ${3:+"/api/todo/$3"}
  "/api/mail/inbox?limit=50"
  "/"
)
printf "%-62s %7s %7s %7s %9s\n" route p50 p95 max bytes
for r in "${routes[@]}"; do
  times=""
  for _ in $(seq 1 "$n"); do
    times="$times $(curl -s --compressed -o /dev/null -w '%{time_total}' "$base$r")"
  done
  bytes=$(curl -s -H 'Accept-Encoding: gzip' -o /dev/null -w '%{size_download}' "$base$r")
  printf '%s\n' $times | sort -n | awk -v r="$r" -v b="$bytes" '{a[NR]=$1*1000} END{printf "%-62s %7.1f %7.1f %7.1f %9d\n", r, a[int(NR*0.5)+1], a[int(NR*0.95)], a[NR], b}'
done
