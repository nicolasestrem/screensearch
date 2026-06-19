#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="x86_64-pc-windows-msvc"
PUBLISH=0
WINDOWS_BUNDLE=0
CLEAN=0
SKIP_CHECKS=0

usage() {
  cat <<'EOF'
Usage: ./scripts/build-release.sh VERSION [options]

Prepare a Windows release from Linux. This script validates the repository,
cross-compiles the Windows Rust executable, and creates a portable ZIP.
The installer, portable ZIP, and checksums are built by the tag-triggered
GitHub Actions release workflow. All ML runs in-process in Rust — there is no
Python sidecar to build or bundle.

Options:
  --publish       Create and push tag vVERSION after validation.
  --windows-bundle
                  Run Windows packaging in GitHub Actions and download it.
  --clean         Remove the cross-compiled release directory first.
  --skip-checks   Skip tests and frontend lint.
  -h, --help      Show this help.
EOF
}

if (($# == 0)); then
  usage >&2
  exit 2
fi

if [[ "$1" == "-h" || "$1" == "--help" ]]; then
  usage
  exit 0
fi

VERSION="$1"
shift

while (($#)); do
  case "$1" in
    --publish)
      PUBLISH=1
      ;;
    --windows-bundle)
      WINDOWS_BUNDLE=1
      ;;
    --clean)
      CLEAN=1
      ;;
    --skip-checks)
      SKIP_CHECKS=1
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

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "VERSION must use semantic versioning, for example 0.4.35" >&2
  exit 2
fi

cd "$ROOT"

cargo_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
if [[ "$cargo_version" != "$VERSION" ]]; then
  echo "Cargo.toml version is $cargo_version, not $VERSION" >&2
  exit 1
fi

if ((PUBLISH)) && [[ -n "$(git status --porcelain)" ]]; then
  echo "Release preparation requires a clean worktree" >&2
  exit 1
fi

if ((PUBLISH && WINDOWS_BUNDLE)); then
  echo "Use either --publish or --windows-bundle, not both" >&2
  exit 2
fi

if ! command -v cargo-xwin >/dev/null 2>&1 && ! cargo xwin --version >/dev/null 2>&1; then
  echo "cargo-xwin is required. Install it with: cargo install cargo-xwin" >&2
  exit 1
fi

if ((CLEAN)); then
  rm -rf "target/$TARGET/release"
fi

echo "[1/5] Building embedded dashboard"
(
  cd screensearch-ui
  npm ci
  npm run build
)

if ((!SKIP_CHECKS)); then
  echo "[2/5] Running Linux quality checks"
  cargo check --locked -p screensearch-db -p screensearch-embeddings -p screensearch-api
  cargo test --locked -p screensearch-db -p screensearch-embeddings -p screensearch-api --lib
  (
    cd screensearch-ui
    npm run lint
  )
  python3 -m py_compile evaluation/evaluate.py
else
  echo "[2/5] Skipping quality checks"
fi

echo "[3/5] Cross-compiling Windows application"
cargo xwin build --release --target "$TARGET" --locked

binary="target/$TARGET/release/screensearch.exe"
if [[ ! -f "$binary" ]]; then
  echo "Windows executable was not produced at $binary" >&2
  exit 1
fi

echo "[4/5] Creating Windows core-preview bundle"
if ! command -v zip >/dev/null 2>&1; then
  echo "zip is required to create the preview bundle" >&2
  exit 1
fi

release_dir="target/$TARGET/release"
preview_dir="$release_dir/screensearch-core-preview"
bundle_dir="$release_dir/bundles"
bundle="$bundle_dir/ScreenSearch-v$VERSION-Windows-Core-Preview.zip"
rm -rf "$preview_dir"
mkdir -p "$preview_dir" "$bundle_dir"
cp "$binary" config.toml LICENSE README.md "$preview_dir/"
(
  cd "$preview_dir"
  zip -qr "$ROOT/$bundle" .
)

echo "[5/5] Release preparation complete"
ls -lh "$binary"
ls -lh "$bundle"

if ((WINDOWS_BUNDLE)); then
  if ! command -v gh >/dev/null 2>&1; then
    echo "GitHub CLI is required for --windows-bundle" >&2
    exit 1
  fi
  if ! gh auth status >/dev/null 2>&1; then
    echo "GitHub CLI is not authenticated. Run: gh auth login" >&2
    exit 1
  fi

  branch="$(git branch --show-current)"
  head_sha="$(git rev-parse HEAD)"
  dispatch_started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo
  echo "Triggering Windows bundle build for $branch"
  gh workflow run release.yml \
    --ref "$branch" \
    -f "version=$VERSION" \
    -f publish_release=false

  run_id=""
  for _ in {1..20}; do
    run_id="$(gh run list \
      --workflow release.yml \
      --branch "$branch" \
      --event workflow_dispatch \
      --limit 10 \
      --json databaseId,createdAt,headSha \
      --jq ".[] | select(.headSha == \"$head_sha\" and .createdAt >= \"$dispatch_started\") | .databaseId" \
      | head -1)"
    [[ -n "$run_id" ]] && break
    sleep 3
  done
  if [[ -z "$run_id" ]]; then
    echo "Could not locate the dispatched Windows build" >&2
    exit 1
  fi

  echo
  echo "Watching run $run_id."
  gh run watch "$run_id" --exit-status
  full_bundle_dir="$bundle_dir/windows-full"
  rm -rf "$full_bundle_dir"
  mkdir -p "$full_bundle_dir"
  gh run download "$run_id" \
    --name release-artifacts \
    --dir "$full_bundle_dir"
  echo
  echo "Full Windows artifacts: $full_bundle_dir"
elif ((PUBLISH)); then
  tag="v$VERSION"
  if git rev-parse "$tag" >/dev/null 2>&1; then
    echo "Tag $tag already exists" >&2
    exit 1
  fi
  git tag -a "$tag" -m "Release $tag"
  git push origin "$tag"
  echo
  echo "Published $tag. GitHub Actions will build the Windows"
  echo "installer, portable ZIP, checksums, and draft GitHub release."
else
  echo
  echo "Build and download full Windows artifacts without publishing a tag:"
  echo "  ./scripts/build-release.sh $VERSION --windows-bundle"
  echo "Or publish the release tag:"
  echo "  ./scripts/build-release.sh $VERSION --publish"
fi
