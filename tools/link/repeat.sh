#!/bin/sh
# tools/link/repeat.sh N [ONLY]: run tools/link/twoproc.sh N times (built once, then --no-build; each run on its own
# port) and count passes - for flaky-link hunting. Logs: /tmp/twoproc_<i>.txt
D=$(cd "$(dirname "$0")" && pwd)
N=${1:-10}
O=${2:-trade}
sh "$D/twoproc.sh" > /tmp/twoproc_0.txt 2>&1
echo "build + first run: exit $?"
pass=0
fail=0
i=1
while [ "$i" -le "$N" ]; do
  if ONLY=$O PORT=$((3600 + i)) sh "$D/twoproc.sh" --no-build > "/tmp/twoproc_$i.txt" 2>&1; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    echo "FAIL run $i (/tmp/twoproc_$i.txt)"
    tail -6 "/tmp/twoproc_$i.txt"
  fi
  i=$((i + 1))
done
echo "$O runs: $pass pass, $fail fail"
