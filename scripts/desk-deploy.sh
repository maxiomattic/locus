#!/usr/bin/env bash
# Deploy desk/ to a REAL ship: copy it into the mounted %locus desk, |commit,
# |install if the desk is not installed yet, and wait for every agent in
# desk/desk.bill to serve /<agent>/openapi.json.
#
# Safe for ships with data. It never nukes, never uninstalls, never wipes a
# mount and never creates notes, files or other records. Agents keep their
# state across the commit (Gall reloads them with +on-load). For the smoke
# test harness that does create data, see notes-occ.sh.
#
# Run ON the server, from a checkout of this repo, as a user with sudo:
#   EYRE_URL=http://127.0.0.1 ./scripts/desk-deploy.sh
#
# Env:
#   EYRE_URL        default http://127.0.0.1 (Eyre is port 80)
#   URBIT_PIER      default /var/lib/urbit/pier/ship
#   LOCUS_DESK      Clay desk name, default locus
#   URBIT_BIN       urbit binary, default from PATH
#   LOCUS_SNAPSHOT  1 (default) = take a ZFS snapshot of the pier dataset
#                   first (never destroyed by this script); 0 = skip.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/ship.sh
. "$ROOT/scripts/lib/ship.sh"

need curl
need rsync
need nc
if ! urbit_bin >/dev/null; then
  echo "urbit binary not in PATH; set URBIT_BIN" >&2
  exit 1
fi
if [ -n "${LOCUS_RESET:-}" ]; then
  echo "LOCUS_RESET is ignored here: desk-deploy.sh never wipes state" >&2
fi

wait_eyre
if ! sudo test -S "$SOCK"; then
  echo "conn.sock not at $SOCK; run this on the server (or set URBIT_PIER)" >&2
  exit 1
fi

mapfile -t AGENTS < <(desk_agents)
if [ "${#AGENTS[@]}" -eq 0 ]; then
  echo "no agents in $DESK_SRC/desk.bill" >&2
  exit 1
fi
echo "desk %$LOCUS_DESK agents: ${AGENTS[*]}"

# Tlon desks that ship agents with the same names as ours. This script will
# not uninstall them; the operator decides.
pikes="$(conn_eval '=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(.^(* %gx /(scot %p our)/hood/(scot %da now)/kiln/pikes/noun)))' || true)"
for d in groups landscape notes; do
  if printf '%s\n' "$pikes" | grep -Eq "%${d}[^]]*live"; then
    cat >&2 <<EOF
Tlon desk %$d is installed and live. Its agents clash with %$LOCUS_DESK
(%notes, %contacts). desk-deploy.sh does not uninstall anything. If this ship
should run Locus only, uninstall it yourself in the dojo (agent state is kept):
  |uninstall %$d
then run this script again.
EOF
    exit 1
  fi
done

if [ "${LOCUS_SNAPSHOT:-1}" = 1 ] && sudo sh -c "command -v zfs" >/dev/null 2>&1; then
  dataset="$(sudo zfs list -H -o name "$(dirname "$URBIT_PIER")" 2>/dev/null || true)"
  if [ -n "$dataset" ]; then
    snap="$dataset@locus-desk-deploy-$(date -u +%Y%m%dT%H%M%SZ)"
    echo "zfs snapshot $snap (roll back: sudo zfs rollback $snap, ship stopped)"
    sudo zfs snapshot "$snap"
  else
    echo "no ZFS dataset at $(dirname "$URBIT_PIER"); skipping snapshot"
  fi
fi

if desk_exists; then
  echo "desk %$LOCUS_DESK exists"
else
  # A new desk is only Clay files from %base, not agent data.
  echo "creating %$LOCUS_DESK from %base"
  merge_desk_from_base
  wait_desk_exists
fi
ensure_mounted

copy_desk_src
commit_poke
# Times out (exit 1) if Clay did not pick up a file; nothing is undone.
for a in "${AGENTS[@]}"; do
  wait_committed "$a"
done

running=()
stopped=()
for a in "${AGENTS[@]}"; do
  if agent_running "$a"; then running+=("$a"); else stopped+=("$a"); fi
done
if [ "${#stopped[@]}" -eq 0 ]; then
  echo "all agents already running (${running[*]}); the commit reloads them in place"
else
  echo "not running yet: ${stopped[*]}"
  install_poke
fi

failed=0
for a in "${AGENTS[@]}"; do
  tries=300
  [ "$a" = notes ] && tries=600
  wait_openapi "/$a/openapi.json" "%$a" "$tries" || failed=1
done
if [ "$failed" = 1 ]; then
  echo "some agents are not serving; nothing was uninstalled or wiped. See journalctl -u urbit." >&2
  exit 1
fi
echo "desk %$LOCUS_DESK deployed: ${AGENTS[*]} serving"
