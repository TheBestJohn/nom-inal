# ---- build ----------------------------------------------------------------
# Pinned to the BUILD platform so the compiler always runs natively and
# cross-compiles to the target, rather than the whole toolchain running under
# QEMU. An emulated Rust release build takes the better part of an hour and
# sometimes runs out of memory; cross-compiling costs roughly a native build.
FROM --platform=$BUILDPLATFORM rust:1.94-slim-bookworm AS builder

ARG TARGETPLATFORM

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      pkg-config gcc-aarch64-linux-gnu libc6-dev-arm64-cross \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Translate Docker's platform string into a Rust target triple once, so every
# later step reads the same answer from the same place.
RUN set -eux; \
    case "$TARGETPLATFORM" in \
      linux/amd64) target=x86_64-unknown-linux-gnu ;; \
      linux/arm64) target=aarch64-unknown-linux-gnu ;; \
      *) echo "unsupported platform: $TARGETPLATFORM" >&2; exit 1 ;; \
    esac; \
    echo "$target" > /target; \
    rustup target add "$target"

# `ring`, via rustls, compiles C as well as Rust, so the cross toolchain has to
# be named for the C compiler and not only the linker.
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc

# Copy manifests first and build a dummy binary so the dependency graph lands
# in its own layer. Editing application source then rebuilds in seconds instead
# of recompiling every crate.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
 && cargo build --release --target "$(cat /target)" \
 && rm -rf src

COPY src ./src
COPY migrations ./migrations
# The preview card's font is compiled into the binary: the runtime image below
# has no fonts at all, so a card drawn from a system font would come out blank.
COPY assets ./assets

# Build provenance, reported by /api/v1/health. Declared here and not at the
# top of the stage on purpose: an ARG invalidates every layer after it, and
# the commit changes on every build, so above the dependency build it would
# recompile every crate each time. Left empty they report as null, which is
# the truthful answer for a build nobody stamped.
ARG GIT_SHA
ARG BUILT_AT
ENV NOM_GIT_SHA=$GIT_SHA \
    NOM_BUILT_AT=$BUILT_AT

# Touch main.rs so cargo does not reuse the dummy binary's fingerprint. The
# binary is copied to a fixed path so the runtime stage need not know the
# target triple.
RUN touch src/main.rs \
 && cargo build --release --target "$(cat /target)" \
 && cp "target/$(cat /target)/release/nom-inal" /nom-inal

# ---- runtime --------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home app

COPY --from=builder /nom-inal /usr/local/bin/nom-inal

# Create the photo directory in the IMAGE, owned by the runtime user.
#
# This is load-bearing, not tidiness. When Docker mounts an empty named volume
# at a path, it seeds the volume from the image's directory at that path —
# ownership included. Without this the path does not exist in the image, so
# Docker creates the mountpoint root:root, the process runs as `app`, and its
# first write fails with EACCES. The boot-time writability check then does
# exactly what it should and refuses to start.
RUN mkdir -p /data/photos && chown -R app:app /data/photos

USER app
WORKDIR /home/app

ENV BIND_ADDR=0.0.0.0:8080
# Matches the directory created above, so the image is correct when run on its
# own and not only under the compose file.
ENV PHOTO_DIR=/data/photos
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=5 \
  CMD curl -fsS http://127.0.0.1:8080/api/v1/health || exit 1

ENTRYPOINT ["/usr/local/bin/nom-inal"]
