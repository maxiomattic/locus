# ZFS on the household server (M1)

Disko in this flake is the source of truth. Do not `zfs create` by hand. Ship boot: [urbit.md](urbit.md).

## Datasets

Pool `rpool` on the **server** disk only:

| Dataset | Mount | Snapshots |
|---|---|---|
| `rpool/root` | `/` | yes |
| `rpool/nix` | `/nix` | no |
| `rpool/urbit` | (not mounted) | no |
| `rpool/urbit/pier` | `/var/lib/urbit/pier` | yes |

Set `digitalWorkspace.server.diskDevice` to that machine’s `/dev/disk/by-id/...` before any Disko format. The flake’s example configuration uses a placeholder path so it will not match a real disk.

Disko only lists that one disk. It will not format other disks. Format **will** wipe `diskDevice`.

## Apply (server, empty/dedicated disk)

1. Point `diskDevice` at the real server disk in the NixOS config that imports `nixosModules.server`.
2. On **that** machine, install/rebuild. First-time partition + pool:

   ```bash
   sudo nix run github:nix-community/disko -- --mode format,mount --flake .#server
   sudo nixos-rebuild switch --flake .#server
   ```

   Use `--mode format,mount` (create if missing). Do **not** pass `destroy` unless the disk is expendable and you intend to wipe it.

3. Confirm:

   ```bash
   zfs list -o name,mountpoint rpool/urbit/pier
   findmnt /var/lib/urbit/pier
   ```

`nixos-rebuild` does not format disks; it mounts what Disko declared. The pier dataset exists after the Disko format/mount (or a later format that creates missing datasets) plus rebuild.

## Snapshots

`services.zfs.autoSnapshot` takes 15-minute / hourly / daily / weekly / monthly snapshots on datasets with `com.sun:auto-snapshot=true` (`rpool/root`, `rpool/urbit/pier`).

Manual snapshot:

```bash
sudo zfs snapshot rpool/urbit/pier@before-change
```

List:

```bash
zfs list -t snapshot -o name,creation rpool/urbit/pier
```

## Rollback

Stop the ship first:

```bash
sudo systemctl stop urbit
sudo zfs rollback rpool/urbit/pier@before-change
sudo systemctl start urbit
```

`rollback` only moves the live dataset to that snapshot. It does not delete newer snapshots unless you pass `-r`. Prefer keeping newer snapshots until you know you do not need them.

## notes-app must not destroy snapshots

Unprivileged processes cannot `zfs destroy`. Do not `zfs allow destroy` for the notes app or its user. Do not give it sudo. Snapshot from autoSnapshot / an admin; never from the GUI.
