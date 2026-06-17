#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${SCREENSEARCH_API_URL:-http://127.0.0.1:3131/api}"

request() {
  local name="$1"
  shift
  echo "Testing $name"
  curl --fail --silent --show-error "$@"
  echo
}

request "health" "$BASE_URL/health"
request "embedding status" "$BASE_URL/embeddings/status"
request "settings" "$BASE_URL/settings"
request "keyword search" "$BASE_URL/search/?q=test&mode=fts"

echo "API verification complete"
