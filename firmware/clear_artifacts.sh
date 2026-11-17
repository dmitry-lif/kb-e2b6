#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  ./clear_artifacts.sh

Removes build artifacts for both standalone projects:
  - kb_firmware/target
  - crs_tools/target

Notes:
  - This is equivalent to "manual clean" and does not invoke Cargo.
  - Must be run from the firmware/ directory.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ "$(pwd)" != "$REPO_ROOT" ]]; then
  echo "error: run from the firmware/ directory: $REPO_ROOT" >&2
  echo "hint: cd \"$REPO_ROOT\" && ./clear_artifacts.sh" >&2
  exit 2
fi

rm -rf \
  "kb_firmware/target" \
  "crs_tools/target"

echo "ok: cleared artifacts (kb_firmware, crs_tools)"
