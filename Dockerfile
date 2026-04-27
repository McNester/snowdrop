FROM rust:1.86-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends clang libclang-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY orchestrator/ orchestrator/
COPY executor/ executor/
COPY wal/ wal/
COPY schema/ schema/
COPY keys/ keys/
COPY sndsql/ sndsql/
COPY tpch-bench/ tpch-bench/
RUN cargo build --release --bin orchestrator

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/orchestrator /usr/local/bin/orchestrator
RUN mkdir -p /data/wal /data/rocks
EXPOSE 5432 8080 9090
ENTRYPOINT ["orchestrator"]
