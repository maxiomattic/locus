# Urbit on the clan server

systemd unit `urbit.service` boots a **comet** (or a planet or moon) on the Disko pier dataset. Do not `zfs create` by hand. Do not put the pier on a non-ZFS disk. Do not install Tlon / `%groups`.

## Layout

| Path | Role |
|---|---|
| `rpool/urbit/pier` → `/var/lib/urbit/pier` | ZFS dataset |
| `/var/lib/urbit/pier/ship` | Vere pier (`.urb`). `-c` cannot target the mountpoint because that directory already exists. |

The unit refuses to start unless `/var/lib/urbit/pier` is that ZFS dataset. User `urbit` owns the mount; the process is not root and is not granted `zfs allow`.

## Apply

Disko first: [zfs.md](zfs.md). Point `locus.server.diskDevice` at a **dedicated server disk** or a **bring-up VM disk** (`/dev/disk/by-id/...` or `/dev/vda`). The flake example uses a placeholder so it will not match a laptop disk.

On that machine:

```bash
sudo nixos-rebuild switch --flake .#server
```

`urbit.service` is wanted by `multi-user.target` (boots after reboot). First comet bootstrap needs outbound HTTPS (pill) and Ames; it can take many minutes.

```bash
systemctl status urbit
journalctl -u urbit -f
```

Ready when the journal shows `http: web interface live` (and usually `bootstrap complete`).

## +code and Eyre on the LAN

Eyre binds `0.0.0.0:80` (firewall opened). From another host on the LAN:

```bash
curl -sS -o /dev/null -w '%{http_code}\n' http://<server-lan-ip>/~/login
```

`200` means Eyre is reachable. Login password is the web code (`+code` in Dojo). On the server, as the pier user:

```bash
sudo -u urbit urbit-code
```

That talks to `conn.sock` (Jael `/j/code`). Do not scrape `/~/login` HTML for the code.

Optional login check (password stays in the file, not the argv):

```bash
sudo -u urbit urbit-code > /run/urbit-web-code
chmod 600 /run/urbit-web-code
curl -sS -o /dev/null -w '%{http_code}\n' -c /tmp/urbit-cookies \
  -X POST http://<server-lan-ip>/~/login \
  --data-urlencode "password=$(cat /run/urbit-web-code)"
rm -f /run/urbit-web-code
```

`204` or a `urbauth-*` cookie is a good login.

## Reboot

```bash
sudo reboot
```

After boot: `systemctl is-active urbit`, then `urbit-code` and the LAN `curl` to `/~/login` again.

## Planet or moon instead of comet

```nix
locus.server.urbit = {
  ship = "planet";                         # or "moon"
  name = "sampel-palnet";                  # @p without the leading ~
  keyFile = "/var/lib/urbit/keys/sampel-palnet.key";
};
```

`keyFile` is a string, not a Nix path, so the key never enters the world-readable Nix store. The file must be absolute and readable by the `urbit` user only. Delete it after the first successful boot; the unit is started with `-` on the read-only path, so it still starts without the file.

## Rollback, and why the pier must not go back in time

Do not roll the pier back and boot it on the network. The network remembers how far the ship got. A pier from an earlier snapshot is behind that, so the ship refuses to start ("double-boot detected"), and the only remedy is a reset to a new identity. The same happens if a copy of the pier boots online anywhere, even once. This is how the first comet was lost on 2026-10-20.

Pier snapshots are still useful:

- To read old data, boot a clone with no network at all: a network namespace with only loopback (`sudo ip netns add offline`, then run `urbit` under `sudo ip netns exec offline`). Never on the host network.
- To bring that data onto the live ship, export it from the offline copy with `locus backup export`, then `locus backup import` it on the live ship (see below).
- A snapshot taken while the ship is stopped, rolled back before the ship starts again, is the same ship at the same point and is safe.

## Reset to a new identity, keeping the data

A comet that will not boot ("double-boot detected", a lost key) is replaced by a new comet. A planet replaces a comet the same way. Notes, folders, files, photos, events and contacts come across; note history, timestamps and messages do not (messages stay in the backup file).

1. Stop the ship and snapshot it: `sudo systemctl stop urbit && sudo zfs snapshot rpool/urbit/pier@pre-reset`.
2. Boot a clone of that snapshot offline (above) and run a gateway beside it in the same namespace (`locus-gateway --listen 127.0.0.1:8190 --eyre http://127.0.0.1:<port> --data-dir <copy of /var/lib/locus/files>`). Get its code with `sudo ip netns exec offline urbit-code <clone>/ship`.
3. In the namespace: `locus connect http://127.0.0.1:8190 --code-file <code>`, then `locus backup export <dir>`. Copy `<dir>` off the machine. Stop the clone.
4. Clear the live pier (`sudo rm -rf /var/lib/urbit/pier/ship`; the snapshot keeps it) and `sudo systemctl start urbit`. With no pier, the unit boots a new comet.
5. Suspend Tlon's `%groups` (it runs agents named `%notes` and `%contacts`), deploy the desk (`./scripts/desk-deploy.sh`), then `locus connect` to the server and `locus backup import <dir>`. Import skips what is already there, so it is safe to rerun.

## notes-app must not destroy snapshots

Unprivileged processes cannot `zfs destroy`. This module does not `zfs allow destroy` for `urbit` or anyone else, and does not install sudoers for `zfs`. Do not add that later for notes-app.

## Bring-up VM

Same Disko config, dedicated **virtual** disk (not the laptop’s NixOS disk, not some other VM’s disk):

```bash
# from this repo; creates ~/vms/locus-server/ (new qcow2)
./scripts/server-vm.sh up
```

That builds an installer ISO, Disko-formats `/dev/vda` inside qemu, boots `urbit.service`, and forwards SSH `127.0.0.1:22221` plus Eyre `127.0.0.1:8080`. Then:

```bash
./scripts/server-vm.sh occ   # rsync this repo and run notes-occ.sh on the guest
```

Manual equivalent: create a VM whose only data disk is expendable, set `diskDevice` to that VM disk (`/dev/vda` in `nixosConfigurations.server-vm`), format/mount with Disko (`--mode format,mount`, not `destroy` unless you intend to wipe that VM disk), then `nixos-rebuild switch --flake .#server-vm`.

Confirm `findmnt -n -o FSTYPE,SOURCE /var/lib/urbit/pier` is `zfs` and `rpool/urbit/pier`. Wait for the unit, then +code and Eyre as above.

Do not point `diskDevice` at this laptop. Do not run Disko against a disk that holds unrelated data.
