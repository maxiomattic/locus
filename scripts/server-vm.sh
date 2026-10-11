#!/usr/bin/env bash
# Bring-up ship in a new quickemu VM with a dedicated qcow2 (Disko ZFS).
# Does not use or touch any other VM's disk.
#
# Layout (same pattern as other quickemu VMs, separate directory):
#   ~/vms/<name>.conf
#   ~/vms/<name>/disk.qcow2
#
# Default name locus-server: SSH 127.0.0.1:22221, Eyre :8080 (notes-app).
# A second test VM can use LOCUS_VM_NAME, LOCUS_SSH_PORT, LOCUS_HTTP_PORT.
# Never reuse another VM's disk.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VMS="${LOCUS_VMS:-$HOME/vms}"
# Default is the notes-app bring-up VM. Agent tests use their own LOCUS_VM_NAME
# so they never share disk, SSH, or Eyre with that guest.
NAME="${LOCUS_VM_NAME:-locus-server}"
VM_USER="locus"
VM_DIR="$VMS/$NAME"
CONF="$VMS/$NAME.conf"
DISK="$VM_DIR/disk.qcow2"
ISO_LINK="$VM_DIR/${NAME}-installer.iso"
SSH_PORT="${LOCUS_SSH_PORT:-22221}"
HTTP_PORT="${LOCUS_HTTP_PORT:-8080}"
SSH_KEY="${LOCUS_SSH_KEY:-$VM_DIR/id_ed25519}"
MARKER="$VM_DIR/installed"
NOTES_VM_DIR="$VMS/locus-server"

need() { command -v "$1" >/dev/null 2>&1 || { echo "need $1" >&2; exit 1; }; }
need quickemu
need ssh
need rsync

ssh_base() {
  ssh -i "$SSH_KEY" \
    -o IdentitiesOnly=yes \
    -o IdentityAgent=none \
    -o StrictHostKeyChecking=accept-new \
    -o UserKnownHostsFile="$VM_DIR/known_hosts" \
    -o BatchMode=yes \
    -o ConnectTimeout=5 \
    -p "$SSH_PORT" \
    "${VM_USER}@127.0.0.1" \
    "$@"
}

write_conf() {
  local with_iso=$1
  mkdir -p "$VM_DIR"
  cat >"$CONF" <<EOF
guest_os="linux"
disk_img="$NAME/disk.qcow2"
ram="4G"
cpu_cores="2"
disk_size="40G"
ssh_port="$SSH_PORT"
port_forwards=("${HTTP_PORT}:80")
public_dir="none"
serial="socket"
EOF
  if [ "$with_iso" = "iso" ]; then
    echo "iso=\"$NAME/${NAME}-installer.iso\"" >>"$CONF"
  fi
}

build_iso() {
  echo "building installer ISO (.#server-vm-iso)"
  nix build --accept-flake-config "$ROOT#server-vm-iso" -o "$VM_DIR/iso-result"
  local src
  src="$(echo "$VM_DIR/iso-result"/iso/*.iso)"
  ln -sfn "$(readlink -f "$src")" "$ISO_LINK"
  echo "ISO $ISO_LINK -> $(readlink -f "$src")"
}

vm_running() {
  local pidfile="$VM_DIR/${NAME}.pid"
  local pid
  [ -f "$pidfile" ] || return 1
  pid="$(cat "$pidfile")"
  kill -0 "$pid" 2>/dev/null
}

wait_poweroff() {
  local pidfile="$VM_DIR/${NAME}.pid"
  local i
  echo "waiting for installer VM to power off"
  for i in $(seq 1 720); do
    if vm_running; then
      sleep 5
      continue
    fi
    if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
      sleep 5
      continue
    fi
    if [ "$i" -lt 6 ]; then
      sleep 5
      continue
    fi
    echo "installer VM exited"
    return
  done
  echo "timeout waiting for installer to finish" >&2
  exit 1
}

# OVMF often drops to the UEFI shell instead of the disk BootOrder.
uefi_boot_disk() {
  local ser="$VM_DIR/${NAME}-serial.socket"
  [ -S "$ser" ] || return 0
  python3 - "$ser" <<'PY' || true
import socket, sys, time
path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(2)
try:
    s.connect(path)
except OSError:
    sys.exit(0)
cmds = [
    b"\r",
    b"FS0:\\EFI\\systemd\\systemd-bootx64.efi\r",
    b"fs0:\\EFI\\systemd\\systemd-bootx64.efi\r",
    b"FS0:\\EFI\\BOOT\\BOOTX64.EFI\r",
    b"fs0:\\EFI\\BOOT\\BOOTX64.EFI\r",
]
try:
    for c in cmds:
        s.sendall(c)
        time.sleep(0.5)
except OSError:
    pass
s.close()
PY
}

ensure_ssh_key() {
  mkdir -p "$VM_DIR"
  if [ -f "$SSH_KEY" ]; then
    return
  fi
  # Installer ISO and #server-vm bake in the notes-VM pubkey. Reuse that
  # private key so a second guest can SSH without rebuilding the ISO for keys.
  if [ -f "$NOTES_VM_DIR/id_ed25519" ] && [ "$SSH_KEY" != "$NOTES_VM_DIR/id_ed25519" ]; then
    cp "$NOTES_VM_DIR/id_ed25519" "$SSH_KEY"
    chmod 600 "$SSH_KEY"
    if [ -f "$NOTES_VM_DIR/id_ed25519.pub" ]; then
      cp "$NOTES_VM_DIR/id_ed25519.pub" "$SSH_KEY.pub"
    fi
    return
  fi
  ssh-keygen -t ed25519 -f "$SSH_KEY" -N "" -C "${NAME}-vm"
}

ensure_iso() {
  mkdir -p "$VM_DIR"
  if [ -e "$ISO_LINK" ]; then
    return
  fi
  build_iso
}

wait_ssh() {
  local i
  echo "waiting for SSH on 127.0.0.1:$SSH_PORT"
  for i in $(seq 1 180); do
    if ssh_base true >/dev/null 2>&1; then
      echo "SSH is up"
      return
    fi
    # Around 45s the firmware has usually reached the UEFI shell.
    if [ "$i" -eq 12 ]; then
      echo "SSH not up yet; trying UEFI fallback boot"
      uefi_boot_disk
    fi
    sleep 5
  done
  echo "timeout waiting for SSH" >&2
  exit 1
}

# Guest generations before PrivateDevices=false cannot |commit unix mounts.
# /etc/systemd/system is the Nix store; apply a runtime drop-in after each boot.
ensure_clay_unix() {
  ssh_base 'sudo mkdir -p /run/systemd/system/urbit.service.d && sudo tee /run/systemd/system/urbit.service.d/clay-unix.conf >/dev/null <<EOF
[Service]
PrivateDevices=false
InaccessiblePaths=/dev/zfs
EOF
need_restart=0
priv=$(systemctl show -p PrivateDevices --value urbit)
if [ "$priv" != "no" ]; then
  need_restart=1
fi
if [ "$need_restart" = 1 ]; then
  sudo systemctl daemon-reload
  sudo systemctl restart urbit
fi
' || true
}

start_vm() {
  write_conf "$1"
  if vm_running; then
    echo "VM already running"
    return
  fi
  # quickemu only attaches iso= on first disk create. Force the installer
  # CD with bootindex=0 so OVMF does not prefer the already-installed ESP.
  local extra=()
  if [ "$1" = "iso" ]; then
    extra+=(
      --extra_args
      "-cdrom ${NAME}/${NAME}-installer.iso -boot order=d"
    )
  fi
  (
    cd "$VMS"
    quickemu --vm "$CONF" --display none --viewer none "${extra[@]}"
  )
}

cmd=${1:-up}

case "$cmd" in
  build-iso)
    mkdir -p "$VM_DIR"
    build_iso
    ;;
  up)
    mkdir -p "$VM_DIR"
    ensure_ssh_key
    ensure_iso
    if [ ! -f "$MARKER" ]; then
      echo "first boot: Disko-format dedicated $DISK and install #server-vm"
      start_vm iso
      wait_poweroff
      touch "$MARKER"
    fi
    echo "booting installed disk (no installer ISO)"
    start_vm disk
    wait_ssh
    ensure_clay_unix
    echo "VM up. SSH: ssh -i $SSH_KEY -p $SSH_PORT ${VM_USER}@127.0.0.1"
    echo "Eyre: http://127.0.0.1:$HTTP_PORT/~/login"
    ;;
  ssh)
    exec ssh -i "$SSH_KEY" \
      -o IdentitiesOnly=yes \
      -o IdentityAgent=none \
      -o StrictHostKeyChecking=accept-new \
      -o UserKnownHostsFile="$VM_DIR/known_hosts" \
      -p "$SSH_PORT" \
      ${VM_USER}@127.0.0.1
    ;;
  rsync)
    wait_ssh
    rsync -a --delete \
      --exclude .git --exclude result --exclude result-* --exclude target \
      --exclude iso-result \
      "$ROOT/" "${VM_USER}@127.0.0.1:locus/" \
      -e "ssh -i $SSH_KEY -o IdentitiesOnly=yes -o IdentityAgent=none -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=$VM_DIR/known_hosts -p $SSH_PORT"
    echo "repo synced to VM ~/locus"
    ;;
    occ)
    "$0" rsync
    ensure_clay_unix
    ssh_base "cd /home/${VM_USER}/locus && LOCUS_RESET=1 EYRE_URL=http://127.0.0.1 ./scripts/notes-occ.sh"
    ;;
  locus)
    "$0" occ
    ssh_base "cd /home/${VM_USER}/locus && EYRE_URL=http://127.0.0.1 ./scripts/locus-check.sh"
    ;;
  photos)
    "$0" occ
    ssh_base "cd /home/${VM_USER}/locus && EYRE_URL=http://127.0.0.1 ./scripts/photos-check.sh"
    ;;
  down)
    if ! vm_running; then
      echo "VM not running"
      exit 0
    fi
    echo "powering off $NAME"
    ssh_base 'sudo poweroff' >/dev/null 2>&1 || true
    for i in $(seq 1 45); do
      if ! vm_running; then
        echo "VM stopped"
        exit 0
      fi
      sleep 2
    done
    echo "poweroff timed out; killing $NAME" >&2
    ( cd "$VMS" && quickemu --vm "$CONF" --kill ) || true
    ;;
  kill)
    if [ -f "$CONF" ]; then
      ( cd "$VMS" && quickemu --vm "$CONF" --kill ) || true
    fi
    ;;
  *)
    echo "usage: $0 [up|build-iso|ssh|rsync|occ|locus|photos|down|kill]" >&2
    exit 1
    ;;
esac
