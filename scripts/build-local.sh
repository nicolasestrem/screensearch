#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="debug"

usage() {
  cat <<'EOF'
Usage: ./scripts/build-local.sh [options]

Build a native Linux development bundle. All ML runs in-process in Rust
(native Windows OCR, fastembed/ONNX embeddings, llama.cpp answer generation) —
there is no Python sidecar to build.

Options:
  --release                Build with Cargo's release profile.
  -h, --help               Show this help.
EOF
}

while (($#)); do
  case "$1" in
    --release)
      PROFILE="release"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

cd "$ROOT"

echo "[1/3] Building embedded dashboard"
(
  cd screensearch-ui
  npm ci
  npm run build
)

echo "[2/3] Building native Linux application ($PROFILE)"
cargo_args=(build --locked)
if [[ "$PROFILE" == "release" ]]; then
  cargo_args+=(--release)
fi
cargo "${cargo_args[@]}"

echo "[3/3] Assembling local bundle"
bundle="$ROOT/target/$PROFILE/screensearch-local"
rm -rf "$bundle"
mkdir -p "$bundle"
cp "target/$PROFILE/screensearch" "$bundle/"
cp config.toml LICENSE README.md "$bundle/"

echo
echo "Local Linux bundle: $bundle"
echo "Run: $bundle/screensearch"
