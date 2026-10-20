# Install a clan server

One procedure takes a fresh x86-64 machine to a working Locus server. That means NixOS on ZFS, a new Urbit comet, the `%locus` desk, and the browser app at `http://<server>:8090/app/`. It erases one disk. Allow about an hour, most of it waiting.

`scripts/fresh-install-test.sh` runs these same steps in a VM and checks the result.

## You need

- The machine, with a disk it can erase and a wired or Wi-Fi network that reaches the internet.
- A USB stick with the NixOS minimal installer ([nixos.org/download](https://nixos.org/download), "Minimal ISO image").
- An SSH key on the computer you administer from (`ssh-keygen -t ed25519` if you have none).

## 1. Boot the installer

Boot the machine from the USB stick. At the prompt, become root and check the network:

```bash
sudo -i
ping -c1 nixos.org        # Wi-Fi: nmtui, then retry
```

## 2. Describe the machine

```bash
nix --extra-experimental-features 'nix-command flakes' \
  flake new -t github:maxiomattic/locus#server /root/clan
ls -l /dev/disk/by-id/                         # find the disk to erase
head -c4 /dev/urandom | od -A none -t x4 | tr -d ' '   # a hostId
nano /root/clan/host.nix
```

In `host.nix`, fill in every line marked `EDIT`. These are the disk, the host name, the host id and your SSH public key.

## 3. Install

Formatting erases the disk named in `host.nix`:

```bash
cd /root/clan
nix --extra-experimental-features 'nix-command flakes' run github:nix-community/disko -- \
  --mode destroy,format,mount --flake .#server
nixos-install --flake .#server --no-root-passwd
mkdir -p /mnt/etc/clan && cp -r /root/clan/. /mnt/etc/clan/
reboot
```

Remove the USB stick when the machine restarts.

## 4. First boot

The machine starts the ship by itself. A new comet mines its identity and syncs, and then `locus-desk.service` installs the `%locus` desk. That takes 5 to 30 minutes. From your computer:

```bash
ssh admin@<server> systemctl is-active urbit locus-gateway locus-desk
```

When all three print `active`, the server works. Get the ship's login code:

```bash
ssh admin@<server> sudo urbit-code
```

Open `http://<server>:8090/app/` and sign in with that code. The desktop apps and the `locus` command line use the same address and code: `locus connect http://<server>:8090 --code-file <file>`.

## 5. Bring data in (optional)

- From another Locus ship: `locus backup export <dir>` there, then `locus backup import <dir>` here. See [urbit.md](urbit.md#reset-to-a-new-identity-keeping-the-data).
- From an Obsidian vault: `locus import-obsidian --url http://<server>:8090 --code-file <file> --notebook <title> <vault>`.

## Later changes

The machine's configuration is in `/etc/clan`. Edit it, then:

```bash
sudo nixos-rebuild switch --flake /etc/clan#server
```

To take a newer Locus: `cd /etc/clan && sudo nix flake update locus`, then rebuild. The desk updates itself after the rebuild, and it snapshots the pier first.

Snapshots and recovery: [zfs.md](zfs.md). Never boot an old copy of the pier on the network; see [urbit.md](urbit.md#rollback-and-why-the-pier-must-not-go-back-in-time).
