#!/usr/bin/env bash
# P3: COSMIC client evaluates — cosmic-comp compositor, autotile defaults,
# Nix-declared browser. Does not nixos-rebuild this laptop. Does not start P4.
#
# Env: NIX extra args via NIX_FLAGS (default --accept-flake-config).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NIX_FLAGS=(${NIX_FLAGS---accept-flake-config})
CFG="$ROOT#nixosConfigurations.client.config"

nix_eval_raw() {
  nix eval "${NIX_FLAGS[@]}" --raw "$@"
}

nix_eval_json() {
  nix eval "${NIX_FLAGS[@]}" --json "$@"
}

echo "evaluating nixosConfigurations.client (COSMIC, not GNOME, not a TWM swap)"

cosmic="$(nix_eval_json "$CFG.services.desktopManager.cosmic.enable")"
greeter="$(nix_eval_json "$CFG.services.displayManager.cosmic-greeter.enable")"
if [ "$cosmic" != "true" ] || [ "$greeter" != "true" ]; then
  echo "COSMIC desktop/greeter must be enabled (got cosmic=$cosmic greeter=$greeter)" >&2
  exit 1
fi
echo "COSMIC desktop + cosmic-greeter enabled"

comp="$(nix_eval_raw "$ROOT#nixosConfigurations.client.pkgs.cosmic-comp.pname")"
if [ "$comp" != "cosmic-comp" ]; then
  echo "compositor package must be cosmic-comp (got $comp)" >&2
  exit 1
fi
echo "pkgs.cosmic-comp is cosmic-comp"

sway="$(nix_eval_json "$CFG.programs.sway.enable")"
hypr="$(nix_eval_json "$CFG.programs.hyprland.enable")"
niri="$(nix_eval_json "$CFG.programs.niri.enable")"
if [ "$sway" != "false" ] || [ "$hypr" != "false" ] || [ "$niri" != "false" ]; then
  echo "Sway/Hyprland/niri must stay off (sway=$sway hyprland=$hypr niri=$niri)" >&2
  exit 1
fi
echo "Sway / Hyprland / niri are off"

fox="$(nix_eval_json "$CFG.programs.firefox.enable")"
if [ "$fox" != "true" ]; then
  echo "browser must be Nix-declared (programs.firefox.enable, got $fox)" >&2
  exit 1
fi
echo "Firefox is Nix-declared (not an Urbit app)"

echo "building locus-cosmic-defaults (autotile)"
out="$(nix build "${NIX_FLAGS[@]}" --no-link --print-out-paths "$ROOT#locus-cosmic-defaults")"
autotile="$(tr -d '[:space:]' <"$out/share/cosmic/com.system76.CosmicComp/v1/autotile")"
behavior="$(tr -d '[:space:]' <"$out/share/cosmic/com.system76.CosmicComp/v1/autotile_behavior")"
if [ "$autotile" != "true" ] || [ "$behavior" != "Global" ]; then
  echo "tiling defaults: want autotile=true Global, got autotile=$autotile behavior=$behavior" >&2
  exit 1
fi
echo "cosmic-comp autotile=true (Global)"

echo "P3 COSMIC check passed"
