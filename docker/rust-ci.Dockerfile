# syntax=docker/dockerfile:1.4
# Rust CI image for rust-psp
# Nightly toolchain with rust-src for mipsel-sony-psp cross-compilation

FROM rust:1.99-slim

# System dependencies (minimal -- no X11/audio/video needed for PSP SDK)
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install a PINNED nightly with rust-src (required for -Zbuild-std on mips).
# The std overlay in rust-std-src/ patches nightly's library sources and
# must match their internal APIs (2026-04-11 reshaped BorrowedCursor:
# `ensure_init -> &mut [u8]`, `advance_checked`). Unpinned nightlies drift
# and break every PSP build (cargo-psp builds std whenever any workspace
# member enables `psp/std`). Bump NIGHTLY_DATE only together with an
# overlay re-sync.
#
# The dated toolchain is symlinked as plain `nightly` so the existing
# `cargo +nightly ...` commands (CI, docs, pre-commit) use it unchanged.
ARG NIGHTLY_DATE=2026-04-13
RUN rustup component add rustfmt clippy \
    && rustup toolchain install "nightly-${NIGHTLY_DATE}" --profile minimal \
        --component rust-src,rustfmt,clippy \
    && host="$(rustc -vV | sed -n 's/^host: //p')" \
    && rm -rf "${RUSTUP_HOME}/toolchains/nightly-${host}" \
    && ln -s "${RUSTUP_HOME}/toolchains/nightly-${NIGHTLY_DATE}-${host}" \
        "${RUSTUP_HOME}/toolchains/nightly-${host}" \
    && rustc +nightly -vV

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
