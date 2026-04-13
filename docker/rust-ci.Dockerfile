# syntax=docker/dockerfile:1.4
# Rust CI image for rust-psp
# Nightly toolchain with rust-src for mipsel-sony-psp cross-compilation

FROM rust:1.93-slim

# System dependencies (minimal -- no X11/audio/video needed for PSP SDK)
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install nightly toolchain with rust-src (required for -Zbuild-std on mips)
# The RUN string includes a date so a nightly-API bump forces cache
# invalidation. `rustup update nightly` on top of `install` makes
# sure rebuilds pick up the latest toolchain even when a prior
# layer had nightly cached. 2026-04-11 reshaped BorrowedCursor
# (`ensure_init -> &mut [u8]`, `advance_checked`); the std overlay
# matches that shape.
ARG NIGHTLY_REFRESH=2026-04-13
RUN echo "nightly refresh ${NIGHTLY_REFRESH}" \
    && rustup install nightly \
    && rustup update nightly \
    && rustup component add rustfmt clippy \
    && rustup component add --toolchain nightly rustfmt rust-src

# Install cargo-deny for license/advisory checks
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install cargo-deny --locked 2>/dev/null || true

# Non-root user (overridden by docker-compose USER_ID/GROUP_ID)
RUN useradd -m -u 1000 ciuser \
    && mkdir -p /tmp/cargo && chmod 1777 /tmp/cargo

WORKDIR /workspace

ENV CARGO_HOME=/tmp/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_INCREMENTAL=1 \
    CARGO_NET_RETRY=10 \
    RUST_BACKTRACE=short

CMD ["bash"]
