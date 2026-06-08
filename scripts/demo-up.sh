#!/usr/bin/env bash
# Cold-start the full Snowdrop demo on the VM and verify every piece.
# Usage:
#   ./scripts/demo-up.sh            # start + verify + warm (crypto)
#   ./scripts/demo-up.sh --tpch     # also warm the TPC-H suite
#   ./scripts/demo-up.sh --no-warm  # start + verify only (skip the ~40s warm)
#   ./scripts/demo-up.sh --status   # just print the health summary
#
# Override any port/db via env (defaults are the VM setup):
#   PG_PORT, SD_PGWIRE, SD_HEALTH, SD_METRICS, DEMO_PORT, DEMO_METRICS,
#   PROM_PORT, GRAFANA_PORT, NEO4J_HTTP, DEMO_DB, REPO
set -uo pipefail

REPO="${REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
PG_PORT="${PG_PORT:-5433}"
SD_PGWIRE="${SD_PGWIRE:-5432}"
SD_HEALTH="${SD_HEALTH:-8080}"
SD_METRICS="${SD_METRICS:-9090}"
DEMO_PORT="${DEMO_PORT:-8095}"
DEMO_METRICS="${DEMO_METRICS:-9091}"
PROM_PORT="${PROM_PORT:-9092}"
GRAFANA_PORT="${GRAFANA_PORT:-3000}"
NEO4J_HTTP="${NEO4J_HTTP:-7474}"
DEMO_DB="${DEMO_DB:-crypto}"
export PGPASSWORD="${PGPASSWORD:-postgres}"

WARM=1; TPCH=0; STATUS_ONLY=0
for a in "$@"; do case "$a" in
  --no-warm) WARM=0 ;; --tpch) TPCH=1 ;; --status) STATUS_ONLY=1 ;;
esac; done

G=$'\e[32m'; R=$'\e[31m'; Y=$'\e[33m'; B=$'\e[1m'; N=$'\e[0m'
ok()   { echo "  ${G}✓${N} $1"; }
bad()  { echo "  ${R}✗${N} $1"; FAIL=1; }
step() { echo; echo "${B}$1${N}"; }

# wait_for "label" timeout_s  <cmd...>   -> returns 0 if cmd succeeds within timeout
wait_for() {
  local label="$1" t="$2"; shift 2
  for ((i=0; i<t; i++)); do "$@" >/dev/null 2>&1 && { ok "$label"; return 0; }; sleep 1; done
  bad "$label (timeout ${t}s)"; return 1
}

pg_ok()   { psql -h 127.0.0.1 -p "$1" -U postgres -d "$2" -c 'SELECT 1;' ; }
http_ok() { curl -sf "$1" ; }

FAIL=0
cd "$REPO" || { echo "repo not found: $REPO"; exit 1; }

if [ "$STATUS_ONLY" -eq 0 ]; then
  step "1/5  Neo4j (docker)"
  docker compose up -d neo4j >/dev/null 2>&1
  wait_for "neo4j  :$NEO4J_HTTP" 60 http_ok "http://localhost:$NEO4J_HTTP"

  step "2/5  PostgreSQL 16 (native)"
  sudo systemctl start postgresql@16-main 2>/dev/null
  wait_for "postgres  :$PG_PORT ($DEMO_DB)" 30 pg_ok "$PG_PORT" "$DEMO_DB"

  step "3/5  Snowdrop orchestrator (native)"
  sudo systemctl reset-failed snowdrop-orchestrator 2>/dev/null
  sudo systemctl start snowdrop-orchestrator 2>/dev/null
  wait_for "snowdrop health  :$SD_HEALTH" 90 bash -c "curl -sf localhost:$SD_HEALTH/health | grep -q ok"
  wait_for "snowdrop pgwire  :$SD_PGWIRE ($DEMO_DB)" 30 pg_ok "$SD_PGWIRE" "$DEMO_DB"

  step "4/5  Demo (native)"
  sudo systemctl reset-failed crypto-demo 2>/dev/null
  sudo systemctl start crypto-demo 2>/dev/null
  wait_for "demo  :$DEMO_PORT" 60 http_ok "http://localhost:$DEMO_PORT/api/symbols"

  step "5/5  Monitoring (docker)"
  docker compose -f docker-compose.monitoring.yml up -d >/dev/null 2>&1
  wait_for "grafana  :$GRAFANA_PORT" 40 http_ok "http://localhost:$GRAFANA_PORT/api/health"
  wait_for "prometheus  :$PROM_PORT" 30 http_ok "http://localhost:$PROM_PORT/-/ready"

  if [ "$WARM" -eq 1 ]; then
    step "Warm caches (so the RAM spike happens now, not on stage)"
    for s in BTC ETH SOL BNB; do
      curl -s "localhost:$DEMO_PORT/api/dashboard?engine=snowdrop&sym=$s&side=ALL" >/dev/null
      curl -s "localhost:$DEMO_PORT/api/dashboard?engine=postgres&sym=$s&side=ALL" >/dev/null
    done
    ok "explorer warmed"
    curl -s -X POST "localhost:$DEMO_PORT/api/benchmark/start?suite=crypto&secs=15" >/dev/null
    echo "  … crypto benchmark warming (18s)"; sleep 18; ok "crypto suite warmed"
    if [ "$TPCH" -eq 1 ]; then
      curl -s -X POST "localhost:$DEMO_PORT/api/benchmark/start?suite=tpch&secs=15" >/dev/null
      echo "  … tpch benchmark warming (18s)"; sleep 18; ok "tpch suite warmed"
    fi
  fi
fi

# ── Final health summary ──────────────────────────────────────────────
step "Health summary"
http_ok "http://localhost:$SD_HEALTH/health" >/dev/null && ok "snowdrop health 200" || bad "snowdrop health"
for svc in snowdrop-orchestrator crypto-demo postgresql@16-main; do
  [ "$(systemctl is-active "$svc" 2>/dev/null)" = active ] && ok "service $svc active" || bad "service $svc not active"
done
docker compose ps neo4j 2>/dev/null | grep -qi 'running\|up' && ok "neo4j container up" || bad "neo4j container"
TG=$(curl -s "localhost:$PROM_PORT/api/v1/targets" 2>/dev/null \
  | python3 -c "import json,sys
try:
  d=json.load(sys.stdin)['data']['activeTargets']
  print(' '.join(f\"{t['labels']['job']}={t['health']}\" for t in d))
except Exception: print('')" 2>/dev/null)
echo "  prometheus targets: ${TG:-<none>}"
echo "$TG" | grep -q 'down' && bad "a prometheus target is down (re-run --status in 5s; may be transient)"
http_ok "http://localhost:$GRAFANA_PORT/api/health" >/dev/null && ok "grafana 200" || bad "grafana"
free -h | awk 'NR==2{printf "  RAM used %s / %s, available %s\n",$3,$2,$7}'
free -h | awk 'NR==3{printf "  swap used %s / %s\n",$3,$2}'

echo
if [ "${FAIL:-0}" -eq 0 ]; then
  echo "${G}${B}  ✅  ALL SYSTEMS GO — demo at http://localhost:$DEMO_PORT${N}"
  exit 0
else
  echo "${R}${B}  ⚠️  PROBLEMS FOUND — see ✗ above. journalctl -u <service> -n 20${N}"
  exit 1
fi
