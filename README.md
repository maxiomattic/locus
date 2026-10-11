# Locus

Locus is a digital ecosystem focused on sovereignty, optimization, and seamlessness.

- Local
- Open source
- Peer-to-peer Urbit ID
- Composable architecture
- Agent-native
- Thin client architecture
- Functional correctness
- Instant performance
- High-efficiency interfaces

A **clan** (a group with one administrator, across any number of homes and machines) runs Locus as its own local cloud, and eventually on its own devices. See [docs/intent.md](docs/intent.md), [docs/architecture.md](docs/architecture.md) and [docs/plan.md](docs/plan.md).

## Status

Early and experimental. Works end to end against a bring-up VM and a real server:

- **Apps:** notes, todo (checkbox lines in notes), calendar, photos, files and contacts, keyboard-first. One shared core (`locus-core`) drives two shells: the browser app (`locus-web`, iced, served at `http://<server>:8090/app/`) and the desktop app (`locus-desktop`, libcosmic, binary `locus`).
- **Server:** a NixOS module with ZFS (Disko), the Urbit ship as a systemd service, and `locus-gateway`, which serves the browser app, stores file and photo bytes by SHA-256, and proxies the ship's API.
- **Ship:** the `%locus` desk with `%notes` (a hard fork of Tlon's), `%calendar`, `%photos`, `%contacts`, `%files`, and the `%locus` command plane (members, named functions, approvals, audit).

This repository is the product flake. Configuration for a particular clan's machines lives in a separate flake that imports these modules:

```nix
{
  inputs.locus.url = "github:maxiomattic/locus";
  # imports = [ inputs.locus.nixosModules.server ];
}
```

## Layout

- `flake.nix`: packages `locus-web`, `locus-desktop`, `locus-gateway`, `notes-app`, `server` (Vere); `nixosModules.server` / `client`. Cheap check: `nix flake check`.
- `nix/`: Disko layout, modules, overlays, pinned tools.
- `desk/`: the `%locus` Urbit desk.
- `crates/`: `locus-core` (app logic), `locus-client` (async API client), `locus-effects` (runs core effects), `locus-web` / `locus-desktop` (shells), `locus-gateway`, plus older blocking clients and CLIs (`notes-*`, `locus-*`, `photos-*`).
- `scripts/`: `desk-deploy.sh` (deploy the desk to a real ship; never wipes data), `web-build.sh`, and test harnesses for the bring-up VM (`server-vm.sh`, `notes-occ.sh`, `locus-check.sh`, `photos-check.sh`).
- `docs/`: intent, architecture, plan, deploy, and one doc per component.

Notes don't install Tlon Messenger or `%groups`.

## Develop

```bash
nix develop
cargo run --release -p notes-app   # debug builds are too slow (iced/wgpu)
```

The window talks to the ship over Eyre, at `http://127.0.0.1:8080` on the bring-up VM. Use **Get code from VM** (or paste the ship's login code), then Connect.

- Versions are saved as you type. Click `vN` to restore one.
- If someone else saved the note first, you're offered keep both, keep mine, or use the ship's copy.
- The `notes` CLI uses the same cache and functions as the window.

## Server

Set `locus.server.diskDevice` to the server's `/dev/disk/by-id/...` before formatting. For snapshots and rollback, see [docs/zfs.md](docs/zfs.md). For the ship, see [docs/urbit.md](docs/urbit.md).

```bash
nix build .#server && ./result/bin/urbit --help

# After Eyre is up on the server: commit the desk and check create/list/conflict
EYRE_URL=http://127.0.0.1 ./scripts/notes-occ.sh
```
