#!/usr/bin/env bash
# Run docs/install.md in a VM, from a blank disk to a working server, and
# check the result.
#
# The VM boots a stock NixOS minimal installer (plus an SSH key so this
# script can type the commands). It gets the host flake from the template
# and runs the documented disko and nixos-install commands. Then it boots the
# installed disk and checks the following:
#   - the ship, the gateway and the desk unit are active
#   - every %locus agent serves
#   - the browser app loads
#   - a login with the ship's code works
#   - a note can be written and read back
#   - a `locus backup` import (optional) recreates the data
#
# Two things differ from a real install. The locus input is this checkout
# (`git archive HEAD`, so commit first), not GitHub. And the system closure
# is built on this host and copied into the VM's /mnt store before
# nixos-install, so the VM does not compile Rust; nixos-install then finds
# it already built. The VM's comet is a real, throwaway identity on the
# network.
#
# Usage: scripts/fresh-install-test.sh [backup-dir]
# Env:   FRESH_DIR (default ~/vms/fresh-install), FRESH_SSH_PORT (22231),
#        FRESH_HTTP_PORT (28090), FRESH_RAM (3G), FRESH_KEEP=1 leaves it running,
#        FRESH_BUILD_ONLY=1 builds the system and stops.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
W="${FRESH_DIR:-$HOME/vms/fresh-install}"
SSH_PORT="${FRESH_SSH_PORT:-22231}"
HTTP_PORT="${FRESH_HTTP_PORT:-28090}"
RAM="${FRESH_RAM:-3G}"
BACKUP="${1:-}"
NIX=(nix --extra-experimental-features 'nix-command flakes')

say() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

mkdir -p "$W"
rm -f "$W/known_hosts"
[ -f "$W/key" ] || ssh-keygen -q -t ed25519 -N '' -C fresh-install-test -f "$W/key"
KEY="$(cat "$W/key.pub")"

vm_ssh() { # vm_ssh USER CMD...
  local user=$1; shift
  ssh -i "$W/key" -o IdentitiesOnly=yes -o IdentityAgent=none -o BatchMode=yes \
    -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile="$W/known_hosts" \
    -o ConnectTimeout=5 -p "$SSH_PORT" "$user@127.0.0.1" "$@"
}
wait_ssh() { # wait_ssh USER
  local i
  for i in $(seq 1 120); do
    vm_ssh "$1" true 2>/dev/null && return 0
    sleep 5
  done
  fail "no SSH as $1 on port $SSH_PORT"
}

say "source: git archive of $(git -C "$ROOT" rev-parse --short HEAD)"
[ -z "$(git -C "$ROOT" status --porcelain)" ] || echo "warning: uncommitted changes are not in the test"
rm -rf "$W/locus" && mkdir -p "$W/locus"
git -C "$ROOT" archive HEAD | tar -x -C "$W/locus"

say "host flake from the template (docs/install.md step 2)"
rm -rf "$W/clan"
"${NIX[@]}" flake new -t "path:$W/locus#server" "$W/clan" >/dev/null
render() { # render LOCUS_PATH: fill in the EDIT lines the way an operator would
  sed -i "s|github:maxiomattic/locus|path:$1|" "$W/clan/flake.nix"
  sed -i \
    -e 's|/dev/disk/by-id/EDIT|/dev/vda|' \
    -e 's|"clan-server"|"fresh-server"|' \
    -e 's|"00000000"|"5eed0001"|' \
    -e "s|\"ssh-ed25519 EDIT\"|\"$KEY\"|" \
    "$W/clan/host.nix"
}
render "$W/locus"
# A VM's disk is virtio with no /dev/disk/by-id; real hardware needs none of this.
python3 - "$W/clan/host.nix" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
s = s.replace('  system.stateVersion', '''  # VM only: virtio disk without /dev/disk/by-id, serial console.
  boot.initrd.availableKernelModules = [ "virtio_pci" "virtio_blk" "virtio_net" ];
  boot.zfs.devNodes = "/dev";
  boot.kernelParams = [ "console=ttyS0,115200n8" ];

  system.stateVersion''')
open(p, 'w').write(s)
PY
grep -q EDIT "$W/clan/host.nix" && fail "host.nix still has EDIT lines"

say "building the system on this host"
TOP="$("${NIX[@]}" build --no-link --print-out-paths "path:$W/clan#nixosConfigurations.server.config.system.build.toplevel")"
echo "system $TOP"
# Build here, run the VM elsewhere: copy $W/key and $W/key.pub to the other
# machine's FRESH_DIR first, so both render the same host.nix.
if [ "${FRESH_BUILD_ONLY:-0}" = 1 ]; then
  echo "$TOP" >"$W/toplevel"
  exit 0
fi

say "installer ISO (stock minimal installer with this test's SSH key)"
ISO="$("${NIX[@]}" build --no-link --print-out-paths --impure --expr "
  let
    f = builtins.getFlake \"path:$W/locus\";
    pkgs = f.inputs.nixpkgs;
  in (pkgs.lib.nixosSystem {
    system = \"x86_64-linux\";
    modules = [
      \"\${pkgs}/nixos/modules/installer/cd-dvd/installation-cd-minimal.nix\"
      {
        users.users.root.openssh.authorizedKeys.keys = [ \"$KEY\" ];
        boot.kernelParams = [ \"console=ttyS0,115200n8\" ];
      }
    ];
  }).config.system.build.isoImage")"
ISO="$(echo "$ISO"/iso/*.iso)"
echo "iso $ISO"

QEMU="$("${NIX[@]}" build --no-link --print-out-paths nixpkgs#qemu_kvm)/bin/qemu-system-x86_64"
OVMF="$("${NIX[@]}" build --no-link --print-out-paths nixpkgs#OVMF.fd)/FV"
QIMG="$(dirname "$QEMU")/qemu-img"

vm_start() { # vm_start [iso]
  local cd=()
  [ "${1:-}" = iso ] && cd=(-cdrom "$ISO" -boot order=d)
  "$QEMU" -enable-kvm -machine q35 -cpu host -smp 2 -m "$RAM" \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF/OVMF_CODE.fd" \
    -drive if=pflash,format=raw,file="$W/vars.fd" \
    -drive file="$W/disk.qcow2",if=virtio,format=qcow2 \
    "${cd[@]}" \
    -nic user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:"$SSH_PORT"-:22,hostfwd=tcp:127.0.0.1:"$HTTP_PORT"-:8090 \
    -display none -serial file:"$W/serial.log" -daemonize -pidfile "$W/qemu.pid"
}
vm_stop() {
  [ -f "$W/qemu.pid" ] || return 0
  local pid
  pid="$(cat "$W/qemu.pid")"
  kill "$pid" 2>/dev/null || true
  for _ in $(seq 1 30); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
  rm -f "$W/qemu.pid"
}
vm_stop
rm -f "$W/disk.qcow2" "$W/vars.fd" "$W/serial.log"
"$QIMG" create -q -f qcow2 "$W/disk.qcow2" 40G
cp "$OVMF/OVMF_VARS.fd" "$W/vars.fd" && chmod u+w "$W/vars.fd"

say "booting the installer"
vm_start iso
wait_ssh root
rsync -a -e "ssh -i $W/key -o IdentitiesOnly=yes -o UserKnownHostsFile=$W/known_hosts -p $SSH_PORT" \
  "$W/locus/" root@127.0.0.1:/root/locus/
rsync -a -e "ssh -i $W/key -o IdentitiesOnly=yes -o UserKnownHostsFile=$W/known_hosts -p $SSH_PORT" \
  "$W/clan/" root@127.0.0.1:/root/clan/
# Same edits as on the host, with the VM's path to the checkout.
vm_ssh root "sed -i 's|path:$W/locus|path:/root/locus|' /root/clan/flake.nix"

say "step 3: disko"
vm_ssh root "cd /root/clan && nix --extra-experimental-features 'nix-command flakes' run github:nix-community/disko -- --mode destroy,format,mount --yes-wipe-all-disks --flake .#server" \
  || fail "disko"

say "copying the built system into the VM's /mnt store"
NIX_SSHOPTS="-i $W/key -o IdentitiesOnly=yes -o UserKnownHostsFile=$W/known_hosts -p $SSH_PORT" \
  "${NIX[@]}" copy --no-check-sigs --to "ssh://root@127.0.0.1?remote-store=local%3Froot%3D%2Fmnt" "$TOP"

say "step 3: nixos-install"
vm_ssh root "cd /root/clan && nixos-install --flake .#server --no-root-passwd && mkdir -p /mnt/etc/clan && cp -r /root/clan/. /mnt/etc/clan/" \
  || fail "nixos-install"
GOT="$(vm_ssh root readlink -f /mnt/nix/var/nix/profiles/system)"
[ "$GOT" = "$TOP" ] || fail "VM installed $GOT, host built $TOP (the evaluations differ)"
vm_ssh root poweroff || true
vm_stop

say "step 4: first boot"
rm -f "$W/known_hosts"
vm_start
wait_ssh admin
for i in $(seq 1 360); do
  st="$(vm_ssh admin 'systemctl is-active urbit locus-gateway locus-desk' 2>/dev/null | tr '\n' ' ')" || true
  [ "$st" = "active active active " ] && break
  case "$st" in *failed*) vm_ssh admin 'journalctl -u locus-desk -u urbit -n 60 --no-pager' || true; fail "a unit failed: $st";; esac
  sleep 10
done
[ "$st" = "active active active " ] || fail "units not active after an hour: $st"
echo "urbit locus-gateway locus-desk: active"

URL="http://127.0.0.1:$HTTP_PORT"
for a in notes locus calendar photos contacts files messages; do
  c="$(curl -s -m 10 -o /dev/null -w '%{http_code}' "$URL/$a/openapi.json")"
  [ "$c" = 200 ] || fail "%$a openapi: HTTP $c"
done
echo "every agent serves"
c="$(curl -s -m 10 -o /dev/null -w '%{http_code}' "$URL/app/")"
[ "$c" = 200 ] || fail "browser app: HTTP $c"
echo "browser app loads"

vm_ssh admin sudo urbit-code >"$W/code"
LOCUS=("$ROOT/target/release/locus")
[ -x "${LOCUS[0]}" ] || LOCUS=(locus)
export XDG_CONFIG_HOME="$W/config"
"${LOCUS[@]}" connect "$URL" --code-file "$W/code"
"${LOCUS[@]}" notes notebook-new "Fresh install check" >/dev/null
nb="$("${LOCUS[@]}" notes notebooks | awk '/Fresh install check/ {print $1}')"
echo "written on a fresh server" | "${LOCUS[@]}" notes new "$nb" "Hello" >/dev/null
ref="$("${LOCUS[@]}" notes ls "$nb" | awk 'NR>1 {print $1; exit}')"
"${LOCUS[@]}" notes show "$ref" | grep -q "written on a fresh server" || fail "note did not read back"
echo "login, write and read a note: ok"

if [ -n "$BACKUP" ]; then
  say "step 5: locus backup import $BACKUP"
  "${LOCUS[@]}" backup import "$BACKUP"
  rm -rf "$W/export"
  "${LOCUS[@]}" backup export "$W/export" >/dev/null
  python3 - "$BACKUP" "$W/export" <<'PY'
import json, sys
def load(p):
    d = json.load(open(p + "/locus-backup.json"))
    notes = set()
    for nb in d["notebooks"]:
        fs = {f["id"]: f for f in nb["folders"]}
        def path(i):
            out = []
            while i in fs and i != nb["root_folder_id"]:
                out.append(fs[i]["name"]); i = fs[i].get("parentFolderId")
            return "/".join(reversed(out))
        for n in nb["notes"]:
            notes.add((nb["title"], path(n["folderId"]), n["title"], n["bodyMd"]))
    files = {(f["dir"], f["name"], f["sha256"]) for f in d["files"]}
    return notes, files
(an, af), (bn, bf) = load(sys.argv[1]), load(sys.argv[2])
missing = (an - bn, af - bf)
print(f"notes {len(an)} in backup, {len(an & bn)} on the server; files {len(af)}, {len(af & bf)}")
sys.exit(1 if any(missing) else 0)
PY
  echo "backup import: every note and file arrived"
fi

say "PASS: a blank disk reached a working server through docs/install.md"
if [ "${FRESH_KEEP:-0}" != 1 ]; then
  vm_ssh admin 'sudo poweroff' 2>/dev/null || true
  vm_stop
fi
