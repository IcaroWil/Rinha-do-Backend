#!/usr/bin/env bash

set -euo pipefail

URL="${1:-http://localhost:9999/fraud-score}"
REQUESTS="${2:-500}"
CONCURRENCY="${3:-4}"

TMP_DIR="/tmp/rinha-payloads"

rm -rf "$TMP_DIR"
mkdir -p "$TMP_DIR"

jq -c '.[]' data/example-payloads.json | awk '{ print > sprintf("/tmp/rinha-payloads/payload-%02d.json", NR-1) }'

echo "Running varied payload test..."
echo "URL: $URL"
echo "Requests: $REQUESTS"
echo "Concurrency: $CONCURRENCY"
echo

seq 1 "$REQUESTS" | xargs -P "$CONCURRENCY" -I {} bash -c '
  file=$(find /tmp/rinha-payloads -name "payload-*.json" | shuf -n 1)
  curl -s -o /dev/null -w "%{http_code} %{time_total}\n" \
    -X POST "'"$URL"'" \
    -H "Content-Type: application/json" \
    --data @"$file"
' | awk '
{
  count++;
  status[$1]++;
  time[count]=$2;
  sum+=$2;
}
END {
  if (count == 0) {
    print "No requests executed";
    exit 1;
  }

  asort(time);

  p50=int(count*0.50); if (p50 < 1) p50=1;
  p90=int(count*0.90); if (p90 < 1) p90=1;
  p95=int(count*0.95); if (p95 < 1) p95=1;
  p99=int(count*0.99); if (p99 < 1) p99=1;

  print "Total requests:", count;
  print "Average:", sum/count "s";
  print "p50:", time[p50] "s";
  print "p90:", time[p90] "s";
  print "p95:", time[p95] "s";
  print "p99:", time[p99] "s";
  print "Status codes:";

  for (code in status) {
    print code, status[code];
  }
}
'
