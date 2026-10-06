# Escape hatch (P6)

Steam and Blender run on the **home server**, outside the Urbit VM, launched through the command plane (`escape.launch`). GPU and `/var/lib/locus/hatch` are shared host resources. The client still renders; it does not host the heavy process. Parent plan: [plan.md](plan.md).

**Done when (this slice):** `%locus` queues `escape.launch` for `steam` / `blender` and rejects unknown apps; `locus hatch` / `locus execute` dry-run unless `LOCUS_HATCH_EXECUTE=1`; Disko has `rpool/locus/hatch`; Nix can install Steam+Blender **only** when `digitalWorkspace.server.hatch.enable` is on. The example `nixosConfigurations.server` leaves hatch **off** so eval does not fetch Steam.

Remote display of a Steam window to a thin client is not this pass.

## Layout

| Path | Role |
|---|---|
| `desk/app/locus.hoon` | `escape.list` / `escape.launch` (queues like `os.rebuild`) |
| `crates/locus-command` | `hatch_argv`, `LOCUS_HATCH_EXECUTE=1` |
| `nix/modules/server.nix` | `hatch.enable`, `hatch.apps`, `tunnel.enable` (M4 pipe) |
| `nix/disko.nix` | `rpool/locus/hatch` → `/var/lib/locus/hatch` |
| `scripts/hatch-p6.sh` | Live queue (does not exec Steam) |

## Check

Automated: `./scripts/plan-check.sh` (hatch stays off).

On the bring-up VM (queues only):

```bash
./scripts/server-vm.sh p6
```

To actually launch on a hatch-enabled server, after owner approve:

```bash
LOCUS_HATCH_EXECUTE=1 locus execute <id>
```

Do not set that on the laptop. Do not enable `hatch.enable` on `nixosConfigurations.server` in this repo’s example.

## M4 tunnel

`digitalWorkspace.server.tunnel.enable` installs `cloudflared`. Credentials stay on the box; the tunnel is a pipe, not a store. Leave it off until off-LAN access is needed.
