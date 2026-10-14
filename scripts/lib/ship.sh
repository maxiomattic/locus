# Shared helpers for scripts that talk to the ship on the server (Eyre over
# HTTP, Hood/Clay through the conn.sock khan thread). Sourced, not run.
#
# Callers set ROOT first. Defaults below match the NixOS server module.
#   EYRE_URL     default http://127.0.0.1 (Eyre is port 80)
#   URBIT_PIER   default /var/lib/urbit/pier/ship
#   LOCUS_DESK   Clay desk name, default locus
#   URBIT_BIN    urbit binary, default from PATH
#
# Nothing in here wipes, nukes or uninstalls anything. The test harness
# (notes-occ.sh) does that itself, behind LOCUS_RESET.

DESK_SRC="$ROOT/desk"
EYRE_URL="${EYRE_URL:-http://127.0.0.1}"
URBIT_PIER="${URBIT_PIER:-/var/lib/urbit/pier/ship}"
LOCUS_DESK="${LOCUS_DESK:-locus}"
SOCK="$URBIT_PIER/.urb/conn.sock"

need() { command -v "$1" >/dev/null 2>&1 || { echo "need $1" >&2; exit 1; }; }

ship_not_up() {
  local script
  script="./scripts/$(basename "$0")"
  cat >&2 <<EOF
Desk fork is in $DESK_SRC (Tlon %notes, MIT, groups/activity/mcp/UI stripped).

Eyre is not reachable at $EYRE_URL/~/login.

To run this script against the live ship:

  1. Finish the bring-up on the clan server (ZFS pier, urbit.service).
     Pier must stay on rpool/urbit/pier — do not boot a fake ship on a
     non-ZFS disk.
  2. From the LAN: curl -sS -o /dev/null -w '%{http_code}\\n' http://<server-lan-ip>/~/login
     200 means Eyre is up.
  3. On that server, from this repo:
       EYRE_URL=http://127.0.0.1 $script

The script calls sudo -u urbit urbit-code itself. Do not paste +code into
the repo or a commit. Do not scrape /~/login HTML for the code.
EOF
  exit 2
}

wait_eyre() {
  local i code
  echo "waiting for Eyre at $EYRE_URL/~/login"
  for i in $(seq 1 60); do
    code=$(curl -sS -o /dev/null -w '%{http_code}' --connect-timeout 5 --max-time 20 \
      "$EYRE_URL/~/login" 2>/dev/null || true)
    if [ "$code" = "200" ]; then
      echo "Eyre is up at $EYRE_URL/~/login"
      return
    fi
    sleep 5
  done
  ship_not_up
}

urbit_bin() {
  if [ -n "${URBIT_BIN:-}" ]; then
    printf '%s\n' "$URBIT_BIN"
    return
  fi
  command -v urbit
}

# Run a strand (Hoon source, no single quotes) on the ship through khan.
conn_eval() {
  local hoon=$1
  local bin nc fyrd
  bin="$(urbit_bin)"
  nc="$(command -v nc)"
  fyrd="[0 %fyrd %base %khan-eval %noun %ted-eval '${hoon}']"
  printf '%s\n' "$fyrd" \
    | sudo -u urbit "$bin" eval -jn 2>/dev/null \
    | sudo -u urbit "$nc" -U -W 60 "$SOCK" \
    | sudo -u urbit "$bin" eval -cn 2>/dev/null
}

hood_poke() {
  local hoon=$1 out
  echo "hood poke" >&2
  out="$(conn_eval "$hoon")" || {
    echo "hood poke failed: $out" >&2
    exit 1
  }
}

# Clay/Hood pokes ack before the vane finishes. Retry scries, not just sleeps.
# khan-eval prints the jammed noun, not dojo pretty-print: %.y is 0 and %.n is 1,
# so never grep for the literal '%.y' in avow output.
wait_until() {
  local desc=$1 hoon=$2 needle=$3
  local i out
  for i in $(seq 1 60); do
    out="$(conn_eval "$hoon" || true)"
    if printf '%s\n' "$out" | grep -Fq "$needle"; then
      echo "$desc"
      return
    fi
    sleep 2
  done
  echo "timeout waiting for $desc" >&2
  echo "$out" >&2
  exit 1
}

# Gall agents the desk starts, from desk.bill (one %name per entry).
desk_agents() {
  grep -o '%[a-z0-9-]*' "$DESK_SRC/desk.bill" | tr -d '%'
}

desk_mount() { printf '%s\n' "$URBIT_PIER/$LOCUS_DESK"; }

# Does Clay have /app/<agent>/hoon on the desk? Emits a %tas so we do not
# grep loobeans or put single-quoted cords inside conn_eval's ted-eval string.
clay_has_app_hoon_scry() {
  local app=$1
  printf '%s' "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/${app}/hoon) %${app}-hoon-yes %${app}-hoon-no)))"
}

clay_has_app() {
  local app=$1 out
  out="$(conn_eval "$(clay_has_app_hoon_scry "$app")" || true)"
  printf '%s\n' "$out" | grep -Fq "${app}-hoon-yes"
}

wait_committed() {
  local app=$1
  wait_until "committed /app/${app}/hoon on %$LOCUS_DESK" \
    "$(clay_has_app_hoon_scry "$app")" \
    "${app}-hoon-yes"
}

desk_exists() {
  local out
  out="$(conn_eval '=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(.^((set desk) %cd /(scot %p our)//(scot %da now))))' || true)"
  printf '%s\n' "$out" | grep -Fq "$LOCUS_DESK"
}

merge_desk_from_base() {
  hood_poke "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  ;<  ~  bind:m  (poke-our %hood kiln-merge+!>([%$LOCUS_DESK our %base da+now %auto]))  (pure:m !>(%ok))"
}

wait_desk_exists() {
  wait_until "desk %$LOCUS_DESK exists" \
    '=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(.^((set desk) %cd /(scot %p our)//(scot %da now))))' \
    "$LOCUS_DESK"
}

mount_desk_poke() {
  hood_poke "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  =/  pax=path  /(scot %p our)/${LOCUS_DESK}/(scot %da now)  ;<  ~  bind:m  (poke-our %hood kiln-mount+!>([pax %${LOCUS_DESK}]))  (pure:m !>(%ok))"
}

# Mount the desk on unix if it is not mounted yet, and wait for the directory.
ensure_mounted() {
  local mount i
  mount="$(desk_mount)"
  if sudo test ! -d "$mount"; then
    echo "mounting %$LOCUS_DESK"
    mount_desk_poke
  else
    echo "mount $mount already present"
  fi
  for i in $(seq 1 90); do
    if sudo test -d "$mount"; then
      break
    fi
    sleep 1
  done
  if sudo test ! -d "$mount"; then
    echo "mount $mount did not appear" >&2
    exit 1
  fi
}

copy_desk_src() {
  local mount
  mount="$(desk_mount)"
  echo "copying $DESK_SRC -> $mount"
  # Clay unix-sync converts by extension; .md / extensionless files nack the commit.
  sudo rsync -a --delete \
    --chown=urbit:urbit \
    --chmod=Du=rwx,Dgo=,Fu=rw,Fgo= \
    --exclude LICENSE.tlon.md \
    --exclude LICENSE.urbit.md \
    --exclude NOTICE \
    --exclude .git \
    "$DESK_SRC/" "$mount/"
  sudo rm -f "$mount/LICENSE.tlon.md" "$mount/LICENSE.urbit.md" "$mount/NOTICE"
}

commit_poke() {
  echo "|commit %$LOCUS_DESK"
  hood_poke "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-commit+!>([%$LOCUS_DESK |]))  (pure:m !>(%ok))"
}

install_poke() {
  echo "|install our %$LOCUS_DESK"
  hood_poke "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  ~  bind:m  (poke-our %hood kiln-install+!>([%$LOCUS_DESK our %$LOCUS_DESK]))  (pure:m !>(%ok))"
}

# Is Gall agent <name> running (on any desk)?
agent_running() {
  local app=$1 out
  out="$(conn_eval "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %gu /(scot %p our)/${app}/(scot %da now)/\$) %${app}-runs-yes %${app}-runs-no)))" || true)"
  printf '%s\n' "$out" | grep -Fq "${app}-runs-yes"
}

# Desk that runs agent $1, or nothing if it is not running.
agent_desk() {
  local app=$1 out
  out="$(conn_eval "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  =/  pre=path  /(scot %p our)/${app}/(scot %da now)  (pure:m !>(?.(.^(? %gu (weld pre /\$)) %agentdesk (cat 3 %agentdesk .^(desk %gd (weld pre /\$))))))" || true)"
  out="$(printf '%s\n' "$out" | grep -o 'agentdesk[a-z0-9-]*' | head -1)"
  out="${out#agentdesk}"
  [ -z "$out" ] || printf '%s\n' "$out"
}

wait_openapi() {
  local path=$1 name=$2 tries=${3:-150}
  local i
  echo "waiting for $name HTTP (openapi)"
  for i in $(seq 1 "$tries"); do
    if [ "$(curl -sS -o /dev/null -w '%{http_code}' --max-time 5 "$EYRE_URL$path" || true)" = "200" ]; then
      echo "$name is serving $path"
      return 0
    fi
    sleep 2
  done
  echo "$name did not come up on $path" >&2
  journalctl -u urbit -n 80 --no-pager >&2 || true
  return 1
}
