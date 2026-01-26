FROM rust:1.83-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    gcc-x86-64-linux-gnu \
    libc6-dev-amd64-cross \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-gnu

ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV OPENSSL_DIR=/usr

COPY Cargo.toml ./
COPY src/local-daemon/Cargo.toml ./src/local-daemon/Cargo.toml
COPY src/remote-server/Cargo.toml ./src/remote-server/Cargo.toml
RUN mkdir -p src/local-daemon/src src/remote-server/src
RUN echo "fn main() {}" > src/local-daemon/src/main.rs
RUN echo "fn main() {}" > src/remote-server/src/main.rs
RUN cargo build --release --target x86_64-unknown-linux-gnu -p remote-server || true
RUN rm -rf src/local-daemon/src src/remote-server/src

COPY . .
RUN touch src/remote-server/src/main.rs
RUN cargo build --release --target x86_64-unknown-linux-gnu -p remote-server

FROM --platform=linux/amd64 debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/x86_64-unknown-linux-gnu/release/remote-server /usr/local/bin/remote-server

ENV STORAGE_PATH=/data
ENV BASE_URL=https://i.lstan.eu
ENV BIND_ADDR=0.0.0.0:3000
ENV MAX_UPLOAD_MB=50

EXPOSE 3000

VOLUME ["/data"]

CMD ["remote-server"]
