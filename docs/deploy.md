# Deploy and test

How to put this code on a machine and prove it. Do **not** `nixos-rebuild switch` a daily-driver desktop with `nixosConfigurations.client` unless that machine should become the Locus COSMIC client. The example server config in this flake is a placeholder.

## Automated checks

```bash
nix flake check --no-build          # flake eval checks
nix develop -c cargo test --workspace --release
./scripts/check.sh                  # photos crates + photos dataset eval
./scripts/cosmic-check.sh           # COSMIC client eval
```

Hoon `test-agent` files under `desk/tests/` run **on the ship** after `|commit`, not in `cargo test`.

## Bring-up VM

If the VM sits in UEFI Shell: `FS0:\>` then `EFI\systemd\systemd-bootx64.efi`.

```bash
./scripts/server-vm.sh up          # SSH 127.0.0.1:22221, Eyre :8080
./scripts/server-vm.sh occ         # rsync repo, |commit desk %locus, OCC checks
./scripts/server-vm.sh locus       # locus command-plane check
./scripts/server-vm.sh photos      # photos + contacts check
./scripts/server-vm.sh ssh         # shell on the guest
./scripts/server-vm.sh down        # graceful poweroff
```

`notes-occ.sh` waits for the `/notes`, `/locus`, `/calendar`, `/photos` and `/contacts` openapi endpoints before it finishes.

## Dedicated server

Import `nixosModules.server` from a **separate** host flake (operator accounts, CalDAV, and binary caches live there, not in this repository). Set these in that host configuration:

```nix
{
  imports = [ locus.nixosModules.server ];

  locus.server = {
    enable = true;
    diskDevice = "/dev/disk/by-id/...";   # Disko formats this disk only
  };

  locus.server.urbit = {
    ship = "planet";
    name = "sampel-palnet";                # @p without the leading ~
    keyFile = "/var/lib/urbit/keys/sampel-palnet.key";
  };
}
```

1. Set `locus.server.diskDevice` to that machine's `/dev/disk/by-id/...`. Disko formats **that disk only**.
2. Put the ship's key file on the server before the first boot, at the `keyFile` path. The file must be readable by the `urbit` user only (owner `urbit`, mode `0400`). Do not put it in the Nix store, in a git repo, or in a `nix` expression as a path literal.
3. Apply with `nixos-rebuild` on that host (or `nixos-install` from the host flake).
4. After the first successful boot (`systemctl status urbit` shows the ship running), delete the key file: `sudo rm /var/lib/urbit/keys/sampel-palnet.key`. The unit still starts without it.
5. On the server: `EYRE_URL=http://127.0.0.1 ./scripts/notes-occ.sh`, then `./scripts/locus-check.sh` and `./scripts/photos-check.sh` as needed.

## COSMIC client

Import `nixosModules.client` on a machine whose disk is the Locus desktop. Prove with `./scripts/cosmic-check.sh` first. Firefox is declared there; it is not an Urbit app.

## Deploy to a real server

1. In the clan flake, update the Locus input and switch: `nix flake update locus && sudo nixos-rebuild switch --flake .#<host>`. This installs the gateway, which serves the browser app on port 8090.
2. On the server, from a checkout of this repo: `EYRE_URL=http://127.0.0.1 ./scripts/desk-deploy.sh`. It snapshots the pier dataset, commits the desk, and waits for every agent. It never wipes or uninstalls anything.
3. Open `http://<server>:8090/app/` and sign in with the ship's code (`sudo urbit-code`).
