FROM rust:1.75-slim-bookworm as builder

WORKDIR /app

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock* ./
COPY src ./src

RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release || true
RUN rm -rf src

COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/remote-server /usr/local/bin/remote-server

ENV STORAGE_PATH=/data
ENV BASE_URL=https://i.lstan.eu
ENV BIND_ADDR=0.0.0.0:3000
ENV MAX_UPLOAD_MB=50

EXPOSE 3000

VOLUME ["/data"]

CMD ["remote-server"]
