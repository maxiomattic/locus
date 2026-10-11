#!/usr/bin/env bash
# Automated bar for the photo/contact crates and the photos ZFS dataset.
# Does not rebuild the laptop and does not boot the bring-up VM.
#
# Env: NIX extra args via NIX_FLAGS (default --accept-flake-config).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NIX_FLAGS=(${NIX_FLAGS---accept-flake-config})
cd "$ROOT"

echo "== cargo test photos-protocol photos-sync =="
nix develop "${NIX_FLAGS[@]}" --command cargo test -p photos-protocol -p photos-sync

echo "== photos ZFS dataset (example server config) =="
photos="$(nix eval "${NIX_FLAGS[@]}" --raw \
  "$ROOT#nixosConfigurations.server.config.fileSystems.\"/var/lib/locus/photos\".mountPoint")"
if [ "$photos" != "/var/lib/locus/photos" ]; then
  echo "want /var/lib/locus/photos, got $photos" >&2
  exit 1
fi
echo "fileSystems.\"/var/lib/locus/photos\".mountPoint=$photos"

snap="$(nix eval "${NIX_FLAGS[@]}" --raw \
  "$ROOT#nixosConfigurations.server.config.fileSystems.\"/var/lib/locus/photos\".device")"
echo "photos dataset device=$snap"

echo "photos check passed"
