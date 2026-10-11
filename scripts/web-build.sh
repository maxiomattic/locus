#!/usr/bin/env bash
# Build the browser shell (crates/locus-web) into dist/web, ready for the
# gateway's --web-root. Paths in the output are relative, so it works under /app/.
#
# Needs the nix dev shell (wasm32 target, wasm-bindgen CLI matching Cargo.lock,
# wasm-opt). Run from anywhere: nix develop -c ./scripts/web-build.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# locus-web is its own workspace (upstream iced, not the libcosmic fork), so
# it is built by manifest. Its artifacts go to the root target dir, which is
# git-ignored.
export CARGO_TARGET_DIR="$ROOT/target"
TARGET=wasm32-unknown-unknown
WASM="$ROOT/target/$TARGET/release/locus_web.wasm"
OUT="$ROOT/dist/web"

# bundle WASM OUT: wasm-bindgen + wasm-opt + index.html from a built
# locus_web.wasm. Shared with the Nix package (packages.locus-web), which runs
# `web-build.sh --bundle` after its own sandboxed cargo build.
bundle() {
  local wasm=$1 out=$2 version
  echo "== wasm-bindgen --target web =="
  rm -rf "$out"
  mkdir -p "$out"
  wasm-bindgen --target web --no-typescript --out-dir "$out" --out-name locus "$wasm"

  echo "== wasm-opt -Oz =="
  wasm-opt -Oz -o "$out/locus_bg.wasm" "$out/locus_bg.wasm"

  echo "== index.html =="
  # crates/locus-web/index.html: focus and shortcut handling; @VERSION@ is the
  # build's content hash for cache busting (?v=).
  version="$(cat "$out/locus.js" "$out/locus_bg.wasm" | sha256sum | cut -c1-12)"
  sed "s/@VERSION@/$version/g" "$ROOT/crates/locus-web/index.html" >"$out/index.html"

  echo "== done (version $version) =="
  du -h "$out/locus_bg.wasm" "$out/locus.js" | sed 's/^/  /'
  echo "$out total: $(du -sh "$out" | cut -f1)"
}

if [ "${1:-}" = "--bundle" ]; then
  [ $# -eq 3 ] || { echo "usage: $0 --bundle <locus_web.wasm> <out-dir>" >&2; exit 2; }
  bundle "$2" "$3"
  exit 0
fi

cd "$ROOT"
echo "== cargo build locus-web ($TARGET, release) =="
cargo build --manifest-path crates/locus-web/Cargo.toml --target "$TARGET" --release

bundle "$WASM" "$OUT"
