#!/usr/bin/env bash
set -euo pipefail

# Install the udev rule that lets the desktop user's browser access the KB-E2B6 over WebHID.
# Linux desktops only (Windows/macOS need nothing). Requires sudo.

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  echo "Usage: ./install-udev-rule.sh   (copies 70-kb-e2b6-webhid.rules to /etc/udev/rules.d and reloads udev)"
  exit 0
fi

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RULE="70-kb-e2b6-webhid.rules"

sudo install -m 0644 "$HERE/$RULE" "/etc/udev/rules.d/$RULE"
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw

echo "ok: installed /etc/udev/rules.d/$RULE — re-plug the KB-E2B6, then reload the configurator page."
