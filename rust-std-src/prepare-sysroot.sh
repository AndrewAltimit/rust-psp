#!/bin/bash
# Prepare a merged sysroot for building Rust std with PSP PAL support.
#
# This script copies the rust-src component to a local directory and overlays
# PSP-specific files on top, creating a complete patched library source that
# can be used with cargo's -Z build-std.
#
# Usage:
#   ./prepare-sysroot.sh [DEST_DIR]
#
# If DEST_DIR is not specified, defaults to target/psp-std-sysroot.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEST="${1:-$REPO_ROOT/target/psp-std-sysroot}"

# Find the nightly sysroot
SYSROOT="$(rustc +nightly --print sysroot 2>/dev/null || rustc --print sysroot)"
RUST_SRC="$SYSROOT/lib/rustlib/src/rust"

if [ ! -d "$RUST_SRC/library" ]; then
    echo "ERROR: rust-src not found at $RUST_SRC/library"
    echo "Install with: rustup component add rust-src --toolchain nightly"
    exit 1
fi

# Check if Cargo.lock exists (validates rust-src is complete)
if [ ! -f "$RUST_SRC/library/Cargo.lock" ]; then
    echo "ERROR: rust-src appears incomplete (no Cargo.lock)"
    exit 1
fi

# Create destination
mkdir -p "$DEST"

# Copy the base rust-src (only if not already present or outdated)
MARKER="$DEST/.psp-sysroot-stamp"
SRC_STAMP="$RUST_SRC/library/Cargo.lock"
OVERLAY_STAMP="$SCRIPT_DIR/library/std/src/sys/pal/psp/mod.rs"

if [ -f "$MARKER" ] && \
   [ "$MARKER" -nt "$SRC_STAMP" ] && \
   [ "$MARKER" -nt "$OVERLAY_STAMP" ]; then
    echo "Sysroot is up-to-date at $DEST"
    exit 0
fi

echo "Preparing PSP std sysroot at $DEST..."

# Clean and copy base source
rm -rf "$DEST/library" "$DEST/src"
cp -a "$RUST_SRC/library" "$DEST/library"

# Copy the src directory if it exists (needed for some build-std configurations)
if [ -d "$RUST_SRC/src" ]; then
    cp -a "$RUST_SRC/src" "$DEST/src"
fi

# Copy Cargo.lock
cp "$RUST_SRC/library/Cargo.lock" "$DEST/Cargo.lock" 2>/dev/null || true

# Overlay PSP-specific files
echo "Overlaying PSP PAL files..."
cp -a "$SCRIPT_DIR/library/"* "$DEST/library/"

# Create timestamp marker
touch "$MARKER"

echo "PSP std sysroot prepared successfully at $DEST"
echo "Files overlaid from $SCRIPT_DIR/library/"
