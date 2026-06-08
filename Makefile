# Snowdrop demo control. Run on the VM (root) from /opt/snowdrop.
.PHONY: up up-tpch start status warm down logs

# Start everything, verify, and warm the crypto cache.
up:
	./scripts/demo-up.sh

# Same, but also warm the TPC-H suite (needs the tpch db loaded).
up-tpch:
	./scripts/demo-up.sh --tpch

# Start + verify only, skip the ~40s warm.
start:
	./scripts/demo-up.sh --no-warm

# Just print the health summary (no start, no warm).
status:
	./scripts/demo-up.sh --status

# Warm caches against an already-running stack.
warm:
	curl -s -X POST "localhost:8095/api/benchmark/start?suite=crypto&secs=15" >/dev/null; sleep 17; echo "warmed"

# Stop the demo cleanly (leaves data on disk).
down:
	-sudo systemctl stop crypto-demo snowdrop-orchestrator
	-docker compose -f docker-compose.monitoring.yml down
	-docker compose stop neo4j
	@echo "stopped (data persisted)"

logs:
	journalctl -u snowdrop-orchestrator -u crypto-demo -n 40 --no-pager
