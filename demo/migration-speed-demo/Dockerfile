FROM rust:1.86-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p migration-speed-demo

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/migration-speed-demo /usr/local/bin/migration-speed-demo
COPY tests/large_postgres_bases_for_test/ticketing_orders/benchmark_queries.json /app/tests/large_postgres_bases_for_test/ticketing_orders/benchmark_queries.json
CMD ["migration-speed-demo"]
