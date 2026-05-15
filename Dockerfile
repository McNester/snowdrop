FROM lukemathwalker/cargo-chef:latest-rust-1.86-bookworm AS chef
RUN apt-get update && apt-get install -y --no-install-recommends clang libclang-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY orchestrator/ orchestrator/
COPY executor/ executor/
COPY wal/ wal/
COPY schema/ schema/
COPY keys/ keys/
COPY sndsql/ sndsql/
COPY tpch-bench/ tpch-bench/
COPY snowdrop-testkit/ snowdrop-testkit/
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY orchestrator/ orchestrator/
COPY executor/ executor/
COPY wal/ wal/
COPY schema/ schema/
COPY keys/ keys/
COPY sndsql/ sndsql/
COPY tpch-bench/ tpch-bench/
COPY snowdrop-testkit/ snowdrop-testkit/
RUN cargo build --release --bin orchestrator

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/orchestrator /usr/local/bin/orchestrator
RUN mkdir -p /data/wal /data/rocks
EXPOSE 5432 8080 9090
ENTRYPOINT ["orchestrator"]

