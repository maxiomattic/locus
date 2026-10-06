{ runCommand }:
# System defaults for cosmic-comp (XDG data `share/cosmic`). User
# `~/.config/cosmic` still wins. Tiling is on globally; compositor stays cosmic-comp.
runCommand "locus-cosmic-defaults" { } ''
  mkdir -p "$out/share/cosmic/com.system76.CosmicComp/v1"
  printf '%s\n' true > "$out/share/cosmic/com.system76.CosmicComp/v1/autotile"
  printf '%s\n' Global > "$out/share/cosmic/com.system76.CosmicComp/v1/autotile_behavior"
  printf '%s\n' true > "$out/share/cosmic/com.system76.CosmicComp/v1/active_hint"
''
