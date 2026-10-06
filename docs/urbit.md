# Urbit on the household server (M2)

systemd unit `urbit.service` boots a **comet** (or optional moon) on the Disko pier dataset. Do not `zfs create` by hand. Do not put the pier on a non-ZFS disk. Do not install Tlon / `%groups` (M3).

## Layout

| Path | Role |
|---|---|
| `rpool/urbit/pier` → `/var/lib/urbit/pier` | ZFS dataset (M1) |
| `/var/lib/urbit/pier/ship` | Vere pier (`.urb`). `-c` cannot target the mountpoint because that directory already exists. |

The unit refuses to start unless `/var/lib/urbit/pier` is that ZFS dataset. User `urbit` owns the mount; the process is not root and is not granted `zfs allow`.

## Apply

Disko first: [zfs.md](zfs.md). Point `digitalWorkspace.server.diskDevice` at a **dedicated server disk** or a **bring-up VM disk** (`/dev/disk/by-id/...` or `/dev/vda`). The flake example uses a placeholder so it will not match a laptop disk.

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

## Moon instead of comet

```nix
digitalWorkspace.server.urbit = {
  ship = "moon";
  moonName = "your-planet-your-moon";
  moonKeyFile = /var/lib/urbit/moon.key;
};
```

## Rollback

```bash
sudo systemctl stop urbit
sudo zfs rollback rpool/urbit/pier@before-change
sudo systemctl start urbit
```

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
