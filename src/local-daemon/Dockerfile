FROM rust:1.83-slim-bookworm as builder

WORKDIR /app

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

COPY . .
RUN touch src/main.rs
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/local-daemon /usr/local/bin/local-daemon

ENV WATCH_FOLDER=/watch
ENV API_URL=http://remote-server:3000/upload
ENV MAX_FOLDER_SIZE_GB=30
ENV FILES_TO_DELETE_ON_CLEANUP=30

VOLUME ["/watch"]

CMD ["local-daemon"]
