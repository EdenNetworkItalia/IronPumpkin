# Builds IronPumpkin from this repository for linux/amd64 and linux/arm64 (musl).
#
# A build needs network access: cargo fetches the crates, and the pumpkin-data build script downloads
# the vanilla server jar from piston-meta.mojang.com and piston-data.mojang.com.
# Build each platform on its own architecture (native runner or QEMU): the Alpine builder is musl
# already, so the compiler needs no cross toolchain.
ARG RUST_VERSION=1.98
ARG ALPINE_VERSION=3.24

FROM rust:${RUST_VERSION}-alpine${ALPINE_VERSION} AS builder

ARG TARGETARCH
# The defaults match the build_release_musl job of rust.yml: a fast build without LTO.
# Pass "true" and "1" to get the release profile of Cargo.toml (fat LTO, one codegen unit).
ARG CARGO_PROFILE_RELEASE_LTO=off
ARG CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16

# git: crates/pumpkin-core/build.rs reads the commit hash from the .git directory.
RUN apk add --no-cache git

WORKDIR /src
COPY . .

# The target directory is a cache mount, so the binary leaves it in the same RUN.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,id=ironpumpkin-target-${TARGETARCH},target=/src/target \
    case "${TARGETARCH}" in \
        "amd64") RUST_TARGET="x86_64-unknown-linux-musl" ;; \
        "arm64") RUST_TARGET="aarch64-unknown-linux-musl" ;; \
        *) echo "Unsupported architecture: ${TARGETARCH}" && exit 1 ;; \
    esac && \
    cargo build --release --locked -p pumpkin --target "${RUST_TARGET}" && \
    cp "target/${RUST_TARGET}/release/pumpkin" /pumpkin-release

FROM alpine:${ALPINE_VERSION}

# Online-mode authentication reads the system root certificates through the platform verifier of reqwest.
RUN apk add --no-cache ca-certificates

COPY --from=builder /pumpkin-release /usr/local/bin/pumpkin

RUN addgroup -g 2613 pumpkin && \
    adduser -u 2613 -G pumpkin -D -h /pumpkin pumpkin && \
    chown -R pumpkin:pumpkin /pumpkin

WORKDIR /pumpkin
USER pumpkin:pumpkin

ENV RUST_BACKTRACE=1
EXPOSE 25565

ENTRYPOINT [ "pumpkin" ]

HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
    CMD nc -z 127.0.0.1 25565 || exit 1
