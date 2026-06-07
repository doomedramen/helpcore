# syntax=docker/dockerfile:1
# Requires Docker BuildKit (default in Docker Desktop and Docker Engine 23+).

# ── Web UI build ───────────────────────────────────────────────────────────────
FROM node:22-alpine AS web-builder

WORKDIR /web
COPY apps/web/package*.json ./
RUN npm ci --ignore-scripts

COPY apps/web/ ./
ENV NEXT_EXPORT=true
RUN npm run build

# ── Rust server build ──────────────────────────────────────────────────────────
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
COPY --from=web-builder /web/out /build/apps/web/out

# Use musl-gcc for bundled C dependencies such as SQLite, but do not set it as
# rustc's linker. musl-gcc cannot link Rust's default static PIE correctly and
# produces an executable that segfaults in musl before main.
# BuildKit cache mounts keep the Cargo registry and incremental artefacts
# across rebuilds so only changed crates are recompiled.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    case "$TARGETARCH" in \
    amd64) \
        CC_x86_64_unknown_linux_musl=musl-gcc \
        HELPCORE_EMBED_WEB_DIR=/build/apps/web/out \
        cargo build --release --target x86_64-unknown-linux-musl -p helpcore-server \
     && cp target/x86_64-unknown-linux-musl/release/helpcore-server /helpcore-server ;; \
    arm64) \
        CC_aarch64_unknown_linux_musl=musl-gcc \
        HELPCORE_EMBED_WEB_DIR=/build/apps/web/out \
        cargo build --release --target aarch64-unknown-linux-musl -p helpcore-server \
     && cp target/aarch64-unknown-linux-musl/release/helpcore-server /helpcore-server ;; \
    esac

# Prove the target executable reaches Rust main. A loader-startup crash emits no
# application error, while a healthy binary reports the deliberately missing
# config file.
RUN output="$(HELPCORE_CONFIG=/__helpcore_smoke_test_missing__.toml \
        /helpcore-server 2>&1 || true)" \
    && printf '%s\n' "$output" \
    && printf '%s\n' "$output" | grep -q "failed to load config"

# ── Runtime ────────────────────────────────────────────────────────────────────
FROM alpine:3

RUN apk add --no-cache ca-certificates su-exec tzdata wget

RUN addgroup -S helpcore && adduser -S helpcore -G helpcore

COPY --from=builder /helpcore-server /usr/local/bin/helpcore-server
COPY docker/default-config.toml /usr/local/share/helpcore/default-config.toml
COPY docker/entrypoint.sh /usr/local/bin/helpcore-entrypoint

RUN chmod 0755 /usr/local/bin/helpcore-entrypoint \
    && mkdir -p /config /data \
    && chown helpcore:helpcore /config /data

ENV HELPCORE_CONFIG=/config/config.toml \
    HELPCORE_DATA=/data

# Verify first-boot initialization and privilege dropping inside the final image.
RUN HELPCORE_CONFIG=/tmp/helpcore-smoke/config.toml \
    HELPCORE_DATA=/tmp/helpcore-smoke/data \
    helpcore-entrypoint sh -c \
        'test -f "$HELPCORE_CONFIG" && test "$(id -u)" = "100"' \
    && rm -rf /tmp/helpcore-smoke

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -q -O /dev/null http://localhost:3000/api/health || exit 1

ENTRYPOINT ["helpcore-entrypoint"]
CMD ["helpcore-server"]
