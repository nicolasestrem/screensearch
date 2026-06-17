#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="debug"
INSTALL_SIDECAR_DEPS=1
BUILD_SIDECAR=1

usage() {
  cat <<'EOF'
Usage: ./scripts/build-local.sh [options]

Build a native Linux development bundle.

Options:
  --release                Build with Cargo's release profile.
  --skip-sidecar           Build only the dashboard and Rust application.
  --skip-sidecar-deps      Reuse already-installed Python dependencies.
  -h, --help               Show this help.
EOF
}

while (($#)); do
  case "$1" in
    --release)
      PROFILE="release"
      ;;
    --skip-sidecar)
      BUILD_SIDECAR=0
      ;;
    --skip-sidecar-deps)
      INSTALL_SIDECAR_DEPS=0
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

echo "[1/4] Building embedded dashboard"
(
  cd screensearch-ui
  npm ci
  npm run build
)

echo "[2/4] Building native Linux application ($PROFILE)"
cargo_args=(build --locked)
if [[ "$PROFILE" == "release" ]]; then
  cargo_args+=(--release)
fi
cargo "${cargo_args[@]}"

if ((BUILD_SIDECAR)); then
  echo "[3/4] Building native Linux PP-OCRv5/Qwen sidecar"
  python_bin="${PYTHON:-python3}"
  if ((INSTALL_SIDECAR_DEPS)); then
    "$python_bin" -m pip install -r sidecar/requirements.txt
  fi
  "$python_bin" sidecar/build.py
else
  echo "[3/4] Skipping quality sidecar"
fi

echo "[4/4] Assembling local bundle"
bundle="$ROOT/target/$PROFILE/screensearch-local"
rm -rf "$bundle"
mkdir -p "$bundle"
cp "target/$PROFILE/screensearch" "$bundle/"
cp config.toml LICENSE README.md "$bundle/"

if ((BUILD_SIDECAR)); then
  sidecar_source="sidecar/dist/screensearch-ai-sidecar"
  if [[ ! -x "$sidecar_source/screensearch-ai-sidecar" ]]; then
    echo "Sidecar executable not found at $sidecar_source/screensearch-ai-sidecar" >&2
    exit 1
  fi
  mkdir -p "$bundle/bin"
  cp -a "$sidecar_source" "$bundle/bin/"
fi

echo
echo "Local Linux bundle: $bundle"
echo "Run: $bundle/screensearch"
