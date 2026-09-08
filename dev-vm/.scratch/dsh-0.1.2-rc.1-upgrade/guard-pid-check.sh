#!/usr/bin/env bash
# Guard used while running the Rust suite: the mock devvm must never touch the hosting
# DSH Runtime. Baseline: pid 12084, pid file mtime 1788638520 (recorded 2026-09-05T20:02:00Z).
pid=$(cat /tmp/devvm-daemon-dsh.pid)
mtime=$(stat -c '%Y' /tmp/devvm-daemon-dsh.pid)
if [[ "$pid" == "12084" && "$mtime" == "1788638520" ]] && kill -0 "$pid" 2>/dev/null; then
  echo "GUARD OK: hosting DSH pid $pid alive, pid file mtime unchanged"
  exit 0
fi
echo "GUARD FAILED: pid=$pid mtime=$mtime"
exit 1
