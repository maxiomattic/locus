# Locus

Household cloud: Urbit-default backend, NixOS + customized COSMIC clients.

The whole plan (vision, M0–M10, P1–P7) is **[docs/plan.md](docs/plan.md)**.

This repository is the **product** flake (desk, notes window, `nixosModules.server`). Household machines — operator accounts, CalDAV, Attic, hardware installers — live in a **separate** NixOS flake. Import the modules from GitHub; do not use a dirty `path:` checkout.

```nix
{
  inputs.locus.url = "github:maxiomattic/locus";
  # imports = [ inputs.locus.nixosModules.server ];
}
```

This repository is through **M3** on the dedicated bring-up VM (`./scripts/server-vm.sh occ`): Clay desk `%locus` is installed, create/list/`%conflict` pass, stock Tlon Messenger is not running. The localhost notes window and `notes` CLI cover M5–M9 against that VM. **P1** (household command plane) is Gall `%locus` on that desk — members, OS approval, audit; see [docs/locus.md](docs/locus.md). **P2** is `%mail` / `%calendar` plus a host ingest bridge; see [docs/mail.md](docs/mail.md). **P3** is a NixOS COSMIC client (`cosmic-comp`, tiling defaults, declared Firefox) — [docs/cosmic.md](docs/cosmic.md), `./scripts/cosmic-p3.sh`. **P4** is the photo catalog + contacts (`%photos` / `%contacts`, ZFS bytes, thumb cache) — [docs/photos.md](docs/photos.md), `./scripts/plan-check.sh`. Tunnel reachability is M4 (not required yet). P5–P7 are not this pass.

## Layout

- `flake.nix` — `notes-app` (libcosmic), `packages.server` (Vere 4.6), `nixosModules.server` (Disko + `urbit.service`; `digitalWorkspace.server.extraDatasets` for host extras), `nixosModules.client` (COSMIC), `nixosModules.mobile` (P5). Cheap eval: `nix flake check` (`server-eval` / `client-eval`).
- `nix/disko.nix` — server disk: `rpool/root`, `rpool/nix`, `rpool/urbit/pier`, `rpool/locus/photos`, `rpool/locus/hatch`
- `docs/plan.md` — canonical Locus plan (vision + M0–M10 + P1–P7)
- `docs/zfs.md` — apply, snapshot, rollback
- `docs/urbit.md` — systemd unit, +code, Eyre on the LAN
- `docs/notes.md` — `%locus` desk, OCC script
- `docs/locus.md` — P1 household command plane (`%locus`)
- `docs/mail.md` — P2 mail and calendar (`%mail` / `%calendar`)
- `docs/cosmic.md` — P3 COSMIC client (`cosmic-comp`, tiling, browser)
- `docs/photos.md` — P4 photo catalog + contacts
- `docs/mobile.md` — P5 stock Graphene / same-ship APIs
- `docs/hatch.md` — P6 Steam/Blender escape hatch
- `docs/deploy.md` — how to deploy and test
- `scripts/notes-occ.sh` — `|commit` our desk; create/list/`%conflict`
- `scripts/locus-p1.sh` — second principal, agent cannot approve `os.rebuild`, purge denied
- `scripts/mail-p2.sh` — mail/calendar create/list/`expectedRevision` conflict + ingest
- `scripts/cosmic-p3.sh` — COSMIC client eval (compositor, autotile, Firefox; not a laptop rebuild)
- `scripts/photos-p4.sh` — photos/contacts create/list/`expectedRevision` conflict
- `scripts/hatch-p6.sh` — queue `escape.launch` (does not exec Steam)
- `scripts/plan-check.sh` — cargo + nix eval for P3–P6 (not a laptop rebuild)
- `crates/notes-protocol` — HTTP client + OCC (`expectedRevision` → conflict)
- `crates/notes-sync` — cache, versions, outbox; `notes` CLI (`cargo run -p notes-sync --bin notes -- help`)
- `crates/notes-app` — COSMIC window (GFM source, graph, preview, right-click delete)
- `crates/locus-protocol` — HTTP client for `%locus`
- `crates/locus-command` — named functions + `locus` CLI (`cargo run -p locus-command --bin locus -- help`)
- `crates/mail-protocol` — HTTP client for `%mail` / `%calendar`
- `crates/mail-bridge` — host RFC822 ingest; dry-run send (`cargo run -p mail-bridge -- ingest scripts/fixtures/p2-ingest.eml`)
- `crates/photos-protocol` — HTTP client for `%photos` / `%contacts`
- `crates/photos-sync` — thumbnail SQLite cache
- `crates/locus-touch` — P5 OpenAPI list (not a COSMIC session)
- `desk/` — Gall desk: `%notes`, `%locus`, `%mail`, `%calendar`, `%photos`, `%contacts` (`desk.bill`), kelvin `[%zuse 408]`

Notes are a hard fork of Tlon `%notes` (MIT) onto **this** desk. This repo does not install Tlon Messenger or `%groups`. `%locus` is the household command plane (P1). `%mail` / `%calendar` are the P2 stores; SMTP/IMAP on the host is a bridge only. `%photos` / `%contacts` are the P4 catalogs (bytes on ZFS).

## Develop

```bash
nix develop
```

Inside the shell: `cargo run --release -p notes-app`. Debug `cargo run` is unusably slow (iced/wgpu).

The window talks to the notes server. For the bring-up VM that is `http://127.0.0.1:8080`. Use **Get code from VM** (or paste the ship login code), then Connect. There is no Save: versions snapshot as you write (word or short pause); click `vN` under the editor to restore. Markdown is GitHub Flavored Markdown in the source (`#` stays visible and dim). Preview and graph are optional. Delete is a right-click on a note. A stale ship save offers keep-both / keep mine / use ship. The `notes` CLI uses the same cache and functions.

## Build the notes window

```bash
nix build .#notes-app
./result/bin/notes-app
```

That binary should open a COSMIC (libcosmic / iced) window.

## Server (M1–M2)

Disko layout and the Urbit unit are for the **household server**, not a laptop. Set `digitalWorkspace.server.diskDevice` to that machine’s `/dev/disk/by-id/...` (or a bring-up VM disk) before format. Snapshot/rollback: [docs/zfs.md](docs/zfs.md). Ship: [docs/urbit.md](docs/urbit.md).

```bash
# Evaluates to rpool/urbit/pier (placeholder disk; will not match a real device)
nix eval .#nixosConfigurations.server.config.fileSystems.\"/var/lib/urbit/pier\".device

# systemd unit is enabled in the example server config
nix eval .#nixosConfigurations.server.config.systemd.services.urbit.wantedBy
```

Vere binary (no pier):

```bash
nix build .#server
./result/bin/urbit --help
```

## Notes desk (M3)

```bash
# on the M2 server, after Eyre is up (docs/urbit.md)
EYRE_URL=http://127.0.0.1 ./scripts/notes-occ.sh
```

That commits `desk/` as `%locus`, logs in with `sudo -u urbit urbit-code`, and checks create/list/`%conflict`. Details: [docs/notes.md](docs/notes.md).
