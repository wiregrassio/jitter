#!/usr/bin/env bash
# Measure Jitter's own footprint on the machine it runs on. Starts Jitter with the stdout sink
# to a file, attaches `perf stat` for the measurement window, and saves both, plus the host
# facts that make a run comparable.
#
# Usage: bench/run.sh <jitter binary> <config.toml> <seconds> <out dir> [cpu]
#   cpu: optional CPU to pin Jitter to (taskset), for isolated-core runs. Combine with
#        `chrt -f 50` around this script for SCHED_FIFO.
#
# Needs perf (linux-tools) and, for hardware counters, perf_event_paranoid <= 1 or root.
set -euo pipefail

bin="${1:?jitter binary}"; config="${2:?config}"; seconds="${3:?seconds}"; out="${4:?out dir}"
cpu="${5:-}"
mkdir -p "$out"

{
  echo "date_utc=$(date -u +%FT%TZ)"
  echo "kernel=$(uname -r)"
  echo "arch=$(uname -m)"
  echo "cpu_model=$(lscpu | awk -F: '/Model name/ {gsub(/^ +/, "", $2); print $2; exit}')"
  echo "cpus=$(nproc)"
  echo "instance_type=$(cat /sys/devices/virtual/dmi/id/product_name 2>/dev/null || echo unknown)"
  echo "cmdline=$(cat /proc/cmdline)"
  echo "jitter=$("$bin" --version) sha256=$(sha256sum "$bin" | cut -c1-16)"
  echo "pinned_cpu=${cpu:-none}"
} > "$out/host.txt"

pin=()
[ -n "$cpu" ] && pin=(taskset -c "$cpu")
"${pin[@]}" "$bin" run "$config" > "$out/records.lp" 2> "$out/jitter.log" &
pid=$!
trap 'kill -TERM "$pid" 2>/dev/null || true' EXIT
# Skip startup and the first partial window.
sleep 3

events=task-clock,context-switches,cpu-migrations,page-faults,cycles,instructions,cache-references,cache-misses,branch-misses
perf stat -x, -e "$events" -p "$pid" -o "$out/perf-stat.csv" -- sleep "$seconds"
# Arm topdown level 1, where this perf has the metric group for the CPU (Neoverse on Graviton).
if perf list metricgroups 2>/dev/null | grep -q TopdownL1; then
  perf stat -x, -M TopdownL1 -p "$pid" -o "$out/perf-topdown.csv" -- sleep "$seconds" \
    || echo "topdown collection failed; perf-topdown.csv is incomplete" >&2
fi

kill -TERM "$pid"
wait "$pid" || { echo "jitter exited nonzero; see $out/jitter.log" >&2; exit 1; }
trap - EXIT
echo "wrote $out"
