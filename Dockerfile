# syntax=docker/dockerfile:1
# Requires Docker BuildKit (default in Docker Desktop and Docker Engine 23+).

FROM rust:1.87-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    musl-tools \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-musl

# Tell the `cc` crate (and Cargo's linker driver) to use musl-gcc when
# compiling or linking for this target.  Without these, bundled C libraries
# (rusqlite/SQLite) are compiled against glibc and the static musl binary
# silently ends up with mixed ABIs or a link failure.
ENV CC_x86_64_unknown_linux_musl=musl-gcc
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc

WORKDIR /build

COPY . .

# BuildKit cache mounts keep the Cargo registry and incremental build artefacts
# across image rebuilds, so only changed crates are recompiled each time.
# The binary is copied out before the cache mount is released.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --target x86_64-unknown-linux-musl -p helpcore-server \
 && cp target/x86_64-unknown-linux-musl/release/helpcore-server /helpcore-server

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
