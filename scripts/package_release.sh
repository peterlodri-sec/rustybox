#!/usr/bin/env bash
# Package release binaries for rustybox and rustybox-core.
#
# Generates standard (with symbols) and slim (stripped) release packages
# compressed with UPX, along with SHA-256 checksum files.
set -euo pipefail

TARGET="${1:-x86_64-unknown-linux-musl}"
OUT_DIR="${2:-dist}"

mkdir -p "$OUT_DIR"

echo "=== Building release binaries for target: $TARGET ==="
cargo build --release --target "$TARGET" --all-features
cargo build --release --target "$TARGET" -p rustybox-core

echo "=== Packaging artifacts into $OUT_DIR ==="

for name in rustybox rustybox-core; do
  BIN="target/$TARGET/release/$name"
  
  if [ ! -f "$BIN" ]; then
    echo "Error: Binary $BIN not found!"
    exit 1
  fi

  # 1. Standard (debug symbols preserved)
  OUT="$OUT_DIR/$name-$TARGET"
  cp "$BIN" "$OUT"
  before=$(stat -c%s "$OUT" 2>/dev/null || stat -f%z "$OUT")
  if command -v upx >/dev/null 2>&1; then
    upx --best --lzma "$OUT" || echo "upx skipped for $OUT"
  fi
  after=$(stat -c%s "$OUT" 2>/dev/null || stat -f%z "$OUT")
  echo "Packed $name (standard): $before -> $after bytes"
  shasum -a 256 "$OUT" | sed "s|$OUT_DIR/||" > "$OUT.sha256"

  # 2. Slim (stripped debug symbols)
  OUT_SLIM="$OUT_DIR/$name-slim-$TARGET"
  cp "$BIN" "$OUT_SLIM"
  strip "$OUT_SLIM" 2>/dev/null || llvm-strip "$OUT_SLIM" 2>/dev/null || echo "strip skipped for $OUT_SLIM"
  before_slim=$(stat -c%s "$OUT_SLIM" 2>/dev/null || stat -f%z "$OUT_SLIM")
  if command -v upx >/dev/null 2>&1; then
    upx --best --lzma "$OUT_SLIM" || echo "upx skipped for $OUT_SLIM"
  fi
  after_slim=$(stat -c%s "$OUT_SLIM" 2>/dev/null || stat -f%z "$OUT_SLIM")
  echo "Packed $name (slim): $before_slim -> $after_slim bytes"
  shasum -a 256 "$OUT_SLIM" | sed "s|$OUT_DIR/||" > "$OUT_SLIM.sha256"
done

echo "=== Release packages generated in $OUT_DIR ==="
ls -la "$OUT_DIR"
