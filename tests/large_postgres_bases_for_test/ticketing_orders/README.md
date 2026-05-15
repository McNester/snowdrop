# Ticketing Orders Large PostgreSQL Fixture

This fixture is the Phase 6.5 large migration candidate.

It is intentionally generated from checked-in SQL instead of a binary dump so it can be recreated in Docker or CI with deterministic mock data.

Profile:
- single-schema `public` OLTP + reporting-friendly ticketing/orders workload
- heavy join paths across customers, events, shows, tickets, orders, payments, promotions, refunds, seat holds, and audit events
- supported-schema subset only: PK, FK, UNIQUE, DEFAULT, narrow CHECK
- realistic skew via hot events, partial refunds, failed payments, and repeat customers

Recommended usage:
- `cargo run -p snowdrop-testkit -- migration-preflight --dump-dir tests/large_postgres_bases_for_test/ticketing_orders --source-database ticketing_orders_large`
- `cargo run -p snowdrop-testkit -- migration-auto --dump-dir tests/large_postgres_bases_for_test/ticketing_orders --source-database ticketing_orders_large`
