#!/usr/bin/env bash
# Run the release binary against the runner's real kernel for a few seconds with the stdout
# sink, then check that each enabled collector produced records. Usage: ci/smoke.sh <binary>
set -euo pipefail

bin="${1:?usage: ci/smoke.sh <path to jitter binary>}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

iface="$(ip route show default | awk '{print $5; exit}')"
root_dev="$(findmnt -no SOURCE / | xargs lsblk -no PKNAME | head -n1)"
[ -n "$root_dev" ] || root_dev="$(findmnt -no SOURCE / | xargs basename)"
pressure=true
if [ ! -r /proc/pressure/cpu ]; then
  pressure=false
  echo "::notice::kernel has no PSI; pressure collector disabled for this run"
fi

sed -e "s/SENTINEL_SET_HOST/ci-$(uname -m)/" \
    -e "s/SENTINEL_SET_INTERFACE/$iface/" \
    -e "s/SENTINEL_SET_DEVICE/$root_dev/" \
    -e 's/^kind = "influxdb"/kind = "stdout"/' \
    deploy/jitter.example.toml | sed '/^\[sink.influxdb\]/,$d' > "$work/jitter.toml"
if [ "$pressure" = false ]; then
  sed -i '/^\[collectors.pressure\]/,/^sample_hz/ s/^enabled = true/enabled = false/' "$work/jitter.toml"
fi

"$bin" check "$work/jitter.toml"
timeout --preserve-status -s TERM 6 "$bin" run "$work/jitter.toml" > "$work/out.lp"

fail=0
for m in system.cpu.utilization_percent system.sched.runqueue_wait_percent \
         system.memory.available_bytes system.network.rx_bytes_per_second \
         system.disk.busy_percent system.clock.max_error_seconds \
         jitter.self.wakeup_lateness_us jitter.self.resident_bytes; do
  n="$(grep -c "^$m," "$work/out.lp" || true)"
  echo "$m: $n records"
  [ "$n" -ge 4 ] || { echo "::error::$m produced $n records, expected at least 4"; fail=1; }
done
if [ "$pressure" = true ]; then
  grep -q '^system.pressure.stall_percent,' "$work/out.lp" || { echo "::error::no pressure records"; fail=1; }
fi

{
  echo "### Smoke run on $(uname -m), kernel $(uname -r)"
  echo '```'
  grep -E '^jitter\.self\.(wakeup_lateness_us|cpu_percent|resident_bytes),' "$work/out.lp" | tail -n 3
  echo '```'
} >> "${GITHUB_STEP_SUMMARY:-/dev/stdout}"
exit "$fail"
