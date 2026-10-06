# Deploy and test

How to put this code on a machine and prove it. Do **not** `nixos-rebuild switch` a daily-driver desktop with `nixosConfigurations.client` unless that machine should become the Locus COSMIC client. Hatch/Steam stays off on the example server config.

## What “done” means in this repo

| Slice | Automated (this machine) | Live (bring-up VM / server) |
|---|---|---|
| P3 COSMIC | `./scripts/cosmic-p3.sh` | Import `nixosModules.client` on a **dedicated** desktop, not the laptop |
| P4 photos/contacts | cargo + Disko eval in `./scripts/plan-check.sh` | `./scripts/server-vm.sh p4` |
| P5 touch APIs | mobile eval in `plan-check.sh` | Phone: stock Graphene → same Eyre URLs (no APK in-repo) |
| P6 hatch | hatch options eval; cargo hatch dry-run | `./scripts/server-vm.sh p6` (queues only) |
| M4 tunnel | `tunnel.enable` is false on the example server | Turn on later; install `cloudflared` only |

One command for the automated bar:

```bash
./scripts/plan-check.sh
```

Hoon `test-agent` files under `desk/tests/` run **on the ship** after `|commit`, not in `cargo test`.

## Bring-up VM (already used for M2–P2)

If the VM sits in UEFI Shell: `FS0:\>` then `EFI\systemd\systemd-bootx64.efi`.

```bash
./scripts/server-vm.sh up          # SSH 127.0.0.1:22221, Eyre :8080
./scripts/server-vm.sh occ         # |commit %locus (now mills photos+contacts too)
./scripts/server-vm.sh p1          # household
./scripts/server-vm.sh p2          # mail/calendar
./scripts/server-vm.sh p4          # photos + contacts OCC
./scripts/server-vm.sh p6          # queue steam; does not exec
```

`notes-occ.sh` waits for `/photos/openapi.json` and `/contacts/openapi.json`. Mill is slower with six agents.

## Dedicated server

Import `nixosModules.server` from `github:maxiomattic/locus` in a **separate** host flake. Operator accounts, CalDAV, and binary caches stay in that flake — not this repository. The example `nixosConfigurations.server` here is a placeholder (Disko will not match a real disk until you set `diskDevice`).

1. Set `digitalWorkspace.server.diskDevice` to that machine’s `/dev/disk/by-id/...`. Disko formats **that disk only**.
2. Leave `hatch.enable` and `tunnel.enable` off until you want Steam/Blender or off-LAN Eyre.
3. Apply with `nixos-rebuild` on that host (or `nixos-install` from the host flake).
4. On the server: `EYRE_URL=http://127.0.0.1 ./scripts/notes-occ.sh`, then `locus-p1.sh`, `mail-p2.sh`, `photos-p4.sh`, `hatch-p6.sh` as needed.
5. Photos bytes: copy files under `/var/lib/locus/photos/...` matching catalog `path`s. Never put `..` in the catalog.

## COSMIC client

Import `nixosModules.client` on a machine whose disk is the Locus desktop. Prove with `./scripts/cosmic-p3.sh` first. Firefox is declared there; it is not an Urbit app.

## Phone

Point a Graphene HTTP client at the same OpenAPI list as `crates/locus-touch` (`/etc/locus/touch-apis.json` if you import `nixosModules.mobile` on some eval host). P7 (AOSP fork) is not started.

## Hatch / GPU

On a hatch-enabled **server** only: `digitalWorkspace.server.hatch.enable = true` (needs unfree Steam). Queue with `locus hatch steam`, owner `locus approve <id>`, then `LOCUS_HATCH_EXECUTE=1 locus execute <id>`. Thin-client streaming of that window is not in this pass.
