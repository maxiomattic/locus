# COSMIC client (P3)

Thoroughly customized COSMIC desktop; **the compositor stays `cosmic-comp`**. Tiling and modal/keyboard density are settings and overlays on that compositor — not Sway, niri, or Hyprland. The browser is ordinary client software declared in Nix; it does not live in Urbit. Parent plan: [plan.md](plan.md).

**Done when (this slice):** `nixosConfigurations.client` enables COSMIC + cosmic-greeter; `pkgs.cosmic-comp` is still `cosmic-comp`; autotile is on globally via `share/cosmic`; Firefox is `programs.firefox`; Sway/Hyprland/niri stay off. Check: [scripts/cosmic-p3.sh](../scripts/cosmic-p3.sh).

This module does **not** switch the GNOME laptop. Import it on a machine (or VM) that should run the Locus desktop. Do not `nixos-rebuild switch --flake .#client` at a disk that holds another OS.

## Layout

| Path | Role |
|---|---|
| `nix/modules/client.nix` | `digitalWorkspace.client` — COSMIC, greeter, browser, tiling defaults |
| `nix/overlays/cosmic.nix` | Patchset overlay (keep cosmic-comp; empty until we have patches) |
| `nix/cosmic-defaults.nix` | System `share/cosmic` autotile / active hint |
| `nixosConfigurations.client` | Evaluable example (placeholder root disk) |

`~/.config/cosmic` on a live session still overrides system defaults.

## Check (no laptop rebuild)

```bash
./scripts/cosmic-p3.sh
```

That is `nix eval` / `nix build` of the example client, not `nixos-rebuild`.

## Browser

`programs.firefox.enable` plus `digitalWorkspace.client.browser` (default `pkgs.firefox`). Wrap flags if you need them; do not hot-swap browser internals. Do not add a Gall “browser” agent.

## Later P3 slices (not this pass)

- Keyboard map: tiling-WM motions on cosmic-comp shortcuts (not a compositor swap)
- Apply the overlay when cosmic-comp needs a real patch
- Session that launches notes-app / `locus` as first-class clients
