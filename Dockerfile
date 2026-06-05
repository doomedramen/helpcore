# syntax=docker/dockerfile:1
# Requires Docker BuildKit (default in Docker Desktop and Docker Engine 23+).

FROM rust:1.87-slim AS builder

ARG TARGETARCH

RUN apt-get update && apt-get install -y --no-install-recommends \
    musl-tools \
    && rm -rf /var/lib/apt/lists/*

RUN case "$TARGETARCH" in \
    amd64) rustup target add x86_64-unknown-linux-musl ;; \
    arm64) rustup target add aarch64-unknown-linux-musl ;; \
    *) echo "Unsupported TARGETARCH: $TARGETARCH" && exit 1 ;; \
    esac

WORKDIR /build

COPY . .

# musl-gcc on amd64 targets x86_64; on arm64 it targets aarch64 natively.
# BuildKit cache mounts keep the Cargo registry and incremental artefacts
# across rebuilds so only changed crates are recompiled.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    case "$TARGETARCH" in \
    amd64) \
        CC_x86_64_unknown_linux_musl=musl-gcc \
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
        cargo build --release --target x86_64-unknown-linux-musl -p helpcore-server \
     && cp target/x86_64-unknown-linux-musl/release/helpcore-server /helpcore-server ;; \
    arm64) \
        CC_aarch64_unknown_linux_musl=musl-gcc \
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
        cargo build --release --target aarch64-unknown-linux-musl -p helpcore-server \
     && cp target/aarch64-unknown-linux-musl/release/helpcore-server /helpcore-server ;; \
    esac

# ── Runtime ────────────────────────────────────────────────────────────────────
FROM alpine:3

RUN apk add --no-cache ca-certificates tzdata wget

RUN addgroup -S helpcore && adduser -S helpcore -G helpcore

COPY --from=builder /helpcore-server /usr/local/bin/helpcore-server

USER helpcore

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -q -O /dev/null http://localhost:3000/health || exit 1

ENTRYPOINT ["helpcore-server"]
