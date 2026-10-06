#!/usr/bin/env bash
# GNOME/COSMIC launcher for notes-app. Starts the bring-up VM if it is down,
# and powers that VM off when the last notes window exits.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="/run/current-system/sw/bin:$HOME/.nix-profile/bin:${PATH:-}"
NIX_FLAGS=(${NIX_FLAGS---accept-flake-config})

NAME="${LOCUS_VM_NAME:-locus-server}"
VMS="${LOCUS_VMS:-$HOME/vms}"
HTTP_PORT="${LOCUS_HTTP_PORT:-8080}"
PIDFILE="$VMS/$NAME/$NAME.pid"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}/org.digital-workspace.notes"
LOG="$DATA/desktop.log"
LOCK="${XDG_RUNTIME_DIR:-/tmp}/locus-notes-desktop.lock"
DESKTOP_DST="${XDG_DATA_HOME:-$HOME/.local/share}/applications/org.digital-workspace.notes.desktop"

mkdir -p "$DATA"

notify() {
  command -v notify-send >/dev/null 2>&1 || return 0
  notify-send --app-name=Notes -i accessories-text-editor "$@" || true
}

vm_running() {
  local pid
  [ -f "$PIDFILE" ] || return 1
  pid="$(cat "$PIDFILE")"
  kill -0 "$pid" 2>/dev/null
}

eyre_up() {
  timeout 1 bash -c "echo >/dev/tcp/127.0.0.1/${HTTP_PORT}" 2>/dev/null
}

install_desktop() {
  local launcher="$ROOT/scripts/notes-desktop.sh"
  mkdir -p "$(dirname "$DESKTOP_DST")"
  cat >"$DESKTOP_DST" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Notes
Comment=Locus notes
Exec=$launcher
TryExec=$launcher
Icon=accessories-text-editor
Terminal=false
Categories=Office;TextEditor;
Keywords=locus;markdown;notes;urbit;
StartupNotify=true
StartupWMClass=notes-app
SingleMainWindow=true
EOF
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$(dirname "$DESKTOP_DST")" >/dev/null 2>&1 || true
  fi
  echo "installed $DESKTOP_DST"
}

if [ "${1:-}" = "--install" ]; then
  install_desktop
  exit 0
fi

exec 9>"$LOCK"
flock 9

stop_vm_if_last() {
  if pgrep -x notes-app >/dev/null 2>&1; then
    echo "$(date -Iseconds) other notes-app still running; leaving VM"
    return
  fi
  echo "$(date -Iseconds) notes-app exited; stopping VM"
  "$ROOT/scripts/server-vm.sh" down || true
}

trap 'stop_vm_if_last >>"$LOG" 2>&1' EXIT

{
  echo "$(date -Iseconds) launch"

  if ! vm_running; then
    notify "Starting the notes VM"
    echo "VM down; ./scripts/server-vm.sh up"
    if ! "$ROOT/scripts/server-vm.sh" up; then
      notify "Notes VM failed to start" "See $LOG"
      exit 1
    fi
  fi

  if ! eyre_up; then
    notify "Waiting for the ship"
    i=0
    while [ "$i" -lt 60 ]; do
      eyre_up && break
      sleep 2
      i=$((i + 1))
    done
    if ! eyre_up; then
      notify "Ship is not answering yet" "Opening Notes anyway"
    fi
  fi
} >>"$LOG" 2>&1

exec 9>&-

cd "$ROOT"
if [ ! -x "$ROOT/target/release/notes-app" ]; then
  notify "Building Notes" "First launch compiles the window"
fi

# Dev-shell libs/icons; use a release binary if cargo already built one.
set +e
nix develop "${NIX_FLAGS[@]}" --command bash -c '
  if [ -x target/release/notes-app ]; then
    exec ./target/release/notes-app
  fi
  exec cargo run --release -p notes-app
' >>"$LOG" 2>&1
status=$?
set -e
exit "$status"
