FROM rust:1.82-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev perl make clang \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

ENV RUST_MIN_STACK=16777216
RUN cargo build --release -p codescope -p codescope-mcp -p codescope-web

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 git \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/codescope /usr/local/bin/
COPY --from=builder /app/target/release/codescope-mcp /usr/local/bin/
COPY --from=builder /app/target/release/codescope-web /usr/local/bin/

RUN mkdir -p /root/.codescope

EXPOSE 9091 3333

ENTRYPOINT ["codescope"]
