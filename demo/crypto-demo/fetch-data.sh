#!/usr/bin/env bash
# Download real Binance spot aggTrades into ./data, one CSV per symbol.
# Public, no auth. Each major symbol-day is ~0.1-1M real trades; several days
# across several symbols gives ~10M real rows.
#
#   DATES="2024-09-02 2024-09-03 ..." ./fetch-data.sh SYM1 SYM2 ...
set -euo pipefail

DATES="${DATES:-2024-09-02 2024-09-03 2024-09-04 2024-09-05 2024-09-06}"
SYMBOLS=("$@")
[ ${#SYMBOLS[@]} -eq 0 ] && SYMBOLS=(BTCUSDT ETHUSDT SOLUSDT BNBUSDT XRPUSDT DOGEUSDT)
OUT="${OUT:-/tmp/binance-data}"
mkdir -p "$OUT"
rm -f "$OUT"/*.csv

for sym in "${SYMBOLS[@]}"; do
  csv="$OUT/$sym.csv"
  for date in $DATES; do
    url="https://data.binance.vision/data/spot/daily/aggTrades/$sym/$sym-aggTrades-$date.zip"
    if curl -fsS -m120 -o "$OUT/$sym-$date.zip" "$url" 2>/dev/null; then
      unzip -oq "$OUT/$sym-$date.zip" -d "$OUT"
      cat "$OUT/$sym-aggTrades-$date.csv" >> "$csv"
      rm -f "$OUT/$sym-$date.zip" "$OUT/$sym-aggTrades-$date.csv"
    else
      echo "$sym $date: skipped (no data)"
    fi
  done
  echo "$sym: $(wc -l < "$csv") trades"
done
echo "done -> $OUT"
