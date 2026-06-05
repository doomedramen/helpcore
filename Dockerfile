FROM rust:1.87-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    musl-tools \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /build

COPY . .

RUN cargo build --release --target x86_64-unknown-linux-musl -p helpcore-server

# ─────────────────────────────────────────────────────────────────────────────

FROM alpine:3

RUN apk add --no-cache ca-certificates tzdata

RUN addgroup -S helpcore && adduser -S helpcore -G helpcore

COPY --from=builder \
    /build/target/x86_64-unknown-linux-musl/release/helpcore-server \
    /usr/local/bin/helpcore-server

USER helpcore

EXPOSE 3000

ENTRYPOINT ["helpcore-server"]
