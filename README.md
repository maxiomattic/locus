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

Early and experimental. What exists today was built and tested against a local bring-up VM:

- **Server:** a NixOS module with a ZFS layout (provisioned with Disko) and an Urbit ship as a systemd service.
- **Notes:** a hard fork of Tlon `%notes` on the `%locus` desk. It has a native libcosmic window and a `notes` CLI that share a local cache, version history and conflict handling.
- **Command plane:** `%locus` provides clan members, named functions, an audit log, and human approval for OS-changing actions. It comes with a `locus` CLI.
- **Early drafts, not in use:** the calendar store and the COSMIC client module.

This repository is the product flake. Configuration for a particular clan's machines lives in a separate flake that imports these modules:

```nix
{
  inputs.locus.url = "github:maxiomattic/locus";
  # imports = [ inputs.locus.nixosModules.server ];
}
```

## Layout

- `flake.nix`: `notes-app`, `packages.server` (Vere), `nixosModules.server` / `client`. Cheap check: `nix flake check`.
- `nix/`: Disko layout, modules, overlays.
- `desk/`: Urbit desk with `%notes`, `%locus`, `%calendar`, `%photos`, `%contacts`.
- `crates/`: Rust clients, `notes-protocol` / `notes-sync` (HTTP client, cache and CLI), `notes-app` (window), `locus-protocol` / `locus-command` (HTTP client and CLI), `photos-protocol` / `photos-sync` (catalog client and thumbnail cache).
- `scripts/`: live checks against the bring-up VM (`server-vm.sh`, `notes-occ.sh`, `locus-check.sh`, `photos-check.sh`) and eval checks (`cosmic-check.sh`, `check.sh`). Flake-level checks: `nix flake check`.
- `docs/`: intent, ZFS, Urbit, deploy, and one doc per component.

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
