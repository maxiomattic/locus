#!/usr/bin/env bash
# |commit Clay desk %locus (Gall %notes / %locus / %calendar / %photos /
# %contacts), then create / list / expectedRevision conflict.
# Does not install Tlon Messenger / %groups.
#
# Env:
#   EYRE_URL          default http://127.0.0.1 (Eyre is port 80)
#   URBIT_PIER        default /var/lib/urbit/pier/ship
#   LOCUS_DESK        Clay desk name, default locus
#   NOTES_CODE_FILE   optional file with +code (never commit this)
#   SKIP_COMMIT       if set, skip Clay install and only run HTTP OCC
#   LOCUS_RESET       1 = wipe %notes/%contacts state and reinstall %locus.
#                     TEST SHIPS ONLY (server-vm.sh sets it). Default 0 keeps data.
#
# +code is read on the server with `sudo -u urbit urbit-code` (or NOTES_CODE_FILE).
# It is never printed, never put in the repo, never passed on argv.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESK_SRC="$ROOT/desk"
EYRE_URL="${EYRE_URL:-http://127.0.0.1}"
URBIT_PIER="${URBIT_PIER:-/var/lib/urbit/pier/ship}"
LOCUS_DESK="${LOCUS_DESK:-locus}"
SOCK="$URBIT_PIER/.urb/conn.sock"

need() { command -v "$1" >/dev/null 2>&1 || { echo "need $1" >&2; exit 1; }; }
need curl
need python3

ship_not_up() {
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
       EYRE_URL=http://127.0.0.1 ./scripts/notes-occ.sh

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

wait_eyre

urbit_bin() {
  if [ -n "${URBIT_BIN:-}" ]; then
    printf '%s\n' "$URBIT_BIN"
    return
  fi
  command -v urbit
}

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

commit_desk() {
  local mount="$URBIT_PIER/$LOCUS_DESK"

  echo "uninstalling Tlon %groups / %landscape / pill %notes / pill %contacts if present (this desk is our fork only)"
  conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%groups))  (pure:m !>(%ok))" >/dev/null || true
  conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%landscape))  (pure:m !>(%ok))" >/dev/null || true
  conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%notes))  (pure:m !>(%ok))" >/dev/null || true
  # Gall dude %contacts is on the pill (Tlon). Nuke it so our %contacts is a
  # fresh install, not a revive of negotiate-%2 state.
  if [ "${LOCUS_RESET:-0}" = 1 ]; then conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-nuke+!>([%notes |]))  (pure:m !>(%ok))" >/dev/null || true; fi
  if [ "${LOCUS_RESET:-0}" = 1 ]; then conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-nuke+!>([%contacts |]))  (pure:m !>(%ok))" >/dev/null || true; fi

  echo "creating/merging %$LOCUS_DESK from %base (auto/init)"
  hood_poke "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  ;<  ~  bind:m  (poke-our %hood kiln-merge+!>([%$LOCUS_DESK our %base da+now %auto]))  (pure:m !>(%ok))"
  wait_until "desk %$LOCUS_DESK exists" \
    '=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(.^((set desk) %cd /(scot %p our)//(scot %da now))))' \
    "$LOCUS_DESK"

  if sudo test ! -d "$mount"; then
    echo "mounting %$LOCUS_DESK"
    hood_poke "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  =/  pax=path  /(scot %p our)/${LOCUS_DESK}/(scot %da now)  ;<  ~  bind:m  (poke-our %hood kiln-mount+!>([pax %${LOCUS_DESK}]))  (pure:m !>(%ok))"
  else
    echo "mount $mount already present"
  fi

  local i
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

  copy_desk_src() {
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

  clay_has_locus() {
    local out
    out="$(conn_eval "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/locus/hoon) %locus-hoon-yes %locus-hoon-no)))" || true)"
    printf '%s\n' "$out" | grep -Fq 'locus-hoon-yes'
  }

  clay_has_photos() {
    local out
    out="$(conn_eval "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/photos/hoon) %photos-hoon-yes %photos-hoon-no)))" || true)"
    printf '%s\n' "$out" | grep -Fq 'photos-hoon-yes'
  }

  clay_has_contacts() {
    local out
    out="$(conn_eval "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/contacts/hoon) %contacts-hoon-yes %contacts-hoon-no)))" || true)"
    printf '%s\n' "$out" | grep -Fq 'contacts-hoon-yes'
  }

  clay_has_files() {
    local out
    out="$(conn_eval "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/files/hoon) %files-hoon-yes %files-hoon-no)))" || true)"
    printf '%s\n' "$out" | grep -Fq 'files-hoon-yes'
  }

  recreate_unix_desk() {
    echo "clay unix missed new files; recreating %$LOCUS_DESK mount"
    conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%$LOCUS_DESK))  (pure:m !>(%ok))" >/dev/null || true
    conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-unmount+!>(%$LOCUS_DESK))  (pure:m !>(%ok))" >/dev/null || true
    sleep 2
    sudo rm -rf "$mount"
    hood_poke "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  ;<  ~  bind:m  (poke-our %hood kiln-merge+!>([%$LOCUS_DESK our %base da+now %auto]))  (pure:m !>(%ok))"
    hood_poke "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  =/  pax=path  /(scot %p our)/${LOCUS_DESK}/(scot %da now)  ;<  ~  bind:m  (poke-our %hood kiln-mount+!>([pax %${LOCUS_DESK}]))  (pure:m !>(%ok))"
    local j
    for j in $(seq 1 90); do
      if sudo test -d "$mount"; then
        return
      fi
      sleep 1
    done
    echo "recreated mount $mount did not appear" >&2
    exit 1
  }

  copy_desk_src

  echo "|commit %$LOCUS_DESK"
  hood_poke "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-commit+!>([%$LOCUS_DESK |]))  (pure:m !>(%ok))"
  # %.y is 0 in the avow noun. Emit a %tas so we do not grep loobeans or
  # put single-quoted cords inside conn_eval's ted-eval string.
  wait_until "committed /app/notes/hoon on %$LOCUS_DESK" \
    "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/notes/hoon) %notes-hoon-yes %notes-hoon-no)))" \
    'notes-hoon-yes'
  if ! clay_has_locus || ! clay_has_photos || ! clay_has_contacts || ! clay_has_files; then
    recreate_unix_desk
    copy_desk_src
    echo "|commit %$LOCUS_DESK (after recreating mount)"
    hood_poke "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-commit+!>([%$LOCUS_DESK |]))  (pure:m !>(%ok))"
    wait_until "committed /app/notes/hoon on %$LOCUS_DESK" \
      "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/notes/hoon) %notes-hoon-yes %notes-hoon-no)))" \
      'notes-hoon-yes'
  fi
  wait_until "committed /app/locus/hoon on %$LOCUS_DESK" \
    "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/locus/hoon) %locus-hoon-yes %locus-hoon-no)))" \
    'locus-hoon-yes'
  wait_until "committed /app/calendar/hoon on %$LOCUS_DESK" \
    "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/calendar/hoon) %calendar-hoon-yes %calendar-hoon-no)))" \
    'calendar-hoon-yes'
  wait_until "committed /app/photos/hoon on %$LOCUS_DESK" \
    "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/photos/hoon) %photos-hoon-yes %photos-hoon-no)))" \
    'photos-hoon-yes'
  wait_until "committed /app/contacts/hoon on %$LOCUS_DESK" \
    "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/contacts/hoon) %contacts-hoon-yes %contacts-hoon-no)))" \
    'contacts-hoon-yes'
  wait_until "committed /app/files/hoon on %$LOCUS_DESK" \
    "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(?:(.^(? %cu /(scot %p our)/${LOCUS_DESK}/(scot %da now)/app/files/hoon) %files-hoon-yes %files-hoon-no)))" \
    'files-hoon-yes'

  # The nukes above stop %notes / %contacts. If %locus is still installed from a
  # previous run, |install is a no-op and those agents stay dead, so uninstall
  # %locus first (desk files stay in Clay).
  if [ "${LOCUS_RESET:-0}" = 1 ]; then conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%$LOCUS_DESK))  (pure:m !>(%ok))" >/dev/null || true; fi
  sleep 5
  echo "|install our %$LOCUS_DESK"
  hood_poke "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  ~  bind:m  (poke-our %hood kiln-install+!>([%$LOCUS_DESK our %$LOCUS_DESK]))  (pure:m !>(%ok))"
  # Do not poke kiln-revive+!>(%locus): this 408k hood nest-fails that vase,
  # bails the khan thread, and can 503 already-running agents. |install plus
  # desk.bill is what starts %notes / %locus after mill succeeds.

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

  wait_openapi /notes/openapi.json %notes 600 || exit 1
  wait_openapi /locus/openapi.json %locus 300 || exit 1
  wait_openapi /calendar/openapi.json %calendar 300 || exit 1
  wait_openapi /photos/openapi.json %photos 300 || exit 1
  wait_openapi /contacts/openapi.json %contacts 300 || exit 1
  wait_openapi /files/openapi.json %files 300 || exit 1
}

if [ -z "${SKIP_COMMIT:-}" ] && sudo test -S "$SOCK"; then
  need rsync
  need nc
  if ! urbit_bin >/dev/null; then
    echo "urbit binary not in PATH; set URBIT_BIN" >&2
    exit 1
  fi
  commit_desk
elif [ -z "${SKIP_COMMIT:-}" ]; then
  echo "conn.sock not at $SOCK — skipping |commit (run this on the server to install the desk)"
  if [ "$(curl -sS -o /dev/null -w '%{http_code}' --max-time 5 "$EYRE_URL/notes/openapi.json" || true)" != "200" ]; then
    echo "%notes is not serving $EYRE_URL/notes/openapi.json yet. Run this script on the server so it can |commit %$LOCUS_DESK." >&2
    exit 1
  fi
fi

CODE_FILE="$(mktemp)"
COOKIES="$(mktemp)"
cleanup() {
  rm -f "$CODE_FILE" "$COOKIES"
}
trap cleanup EXIT
chmod 600 "$CODE_FILE"

# curl --data-urlencode name@FILE includes newlines. +code must be one line.
if [ -n "${NOTES_CODE_FILE:-}" ]; then
  tr -d '\n\r' <"$NOTES_CODE_FILE" >"$CODE_FILE"
elif command -v urbit-code >/dev/null 2>&1; then
  sudo -u urbit urbit-code | tr -d '\n\r' >"$CODE_FILE"
else
  echo "cannot get +code: run on the server (sudo -u urbit urbit-code) or set NOTES_CODE_FILE" >&2
  exit 1
fi
if [ ! -s "$CODE_FILE" ]; then
  echo "empty +code; urbit-code / NOTES_CODE_FILE produced nothing" >&2
  exit 1
fi

# password@FILE keeps +code off argv (do not scrape /~/login HTML).
login_code="$(curl -sS -o /dev/null -w '%{http_code}' -c "$COOKIES" \
  -X POST "$EYRE_URL/~/login" \
  --data-urlencode "password@$CODE_FILE")"
: >"$CODE_FILE"
if [ "$login_code" != "204" ] && ! grep -q urbauth "$COOKIES"; then
  echo "login failed (HTTP $login_code); expected 204 or an urbauth cookie" >&2
  exit 1
fi
echo "logged in to Eyre"

python3 - "$EYRE_URL" "$COOKIES" <<'PY'
import json, subprocess, sys, time

eyre, cookie_file = sys.argv[1], sys.argv[2]

def req(method, path, body=None):
    cmd = [
        "curl", "-sS", "-b", cookie_file, "-c", cookie_file,
        "-w", "\n%{http_code}", "--max-time", "60",
        "-X", method, eyre.rstrip("/") + path,
        "-H", "Accept: application/json",
    ]
    if body is not None:
        cmd += ["-H", "Content-Type: application/json", "-d", json.dumps(body)]
    out = subprocess.check_output(cmd)
    raw, _, code_s = out.rpartition(b"\n")
    code = int(code_s.decode())
    if not raw:
        return code, None
    try:
        return code, json.loads(raw)
    except json.JSONDecodeError:
        raise SystemExit(f"{method} {path} HTTP {code} not JSON: {raw[:500]!r}")

def expect_ok(code, payload, path):
    if code != 200:
        raise SystemExit(f"{path} HTTP {code}: {payload}")
    return payload

def wait_terminal(code, payload, path):
    payload = expect_ok(code, payload, path)
    body = (payload or {}).get("body") or {}
    if body.get("type") != "pending":
        return payload
    rid = payload.get("requestId")
    if not rid:
        raise SystemExit(f"{path} pending with no requestId: {payload}")
    for _ in range(30):
        time.sleep(1)
        code, payload = req("GET", f"/notes/~/v1/request/{rid}")
        payload = expect_ok(code, payload, f"poll {rid}")
        body = (payload or {}).get("body") or {}
        if body.get("type") != "pending":
            return payload
    raise SystemExit(f"{path} still pending after poll: {payload}")

title = "OCC"
code, created = req("POST", "/notes/~/v1/notebooks", {"title": title})
created = wait_terminal(code, created, "create notebook")
body = created.get("body") or {}
if body.get("type") != "notebook":
    raise SystemExit(f"create notebook: expected body.type=notebook, got {created}")
nb = body["notebook"]
host, name = nb["host"], nb["flagName"]
root = nb["notebook"]["rootFolderId"]
base = f"/notes/~/v1/notebooks/{host}/{name}"
print(f"created notebook {host}/{name} rootFolderId={root}")

code, folder = req("POST", f"{base}/folders", {"folderName": "OCC Folder", "parent": root})
folder = wait_terminal(code, folder, "create folder")
if (folder.get("body") or {}).get("type") == "error":
    raise SystemExit(f"create folder error: {folder}")

code, folders = req("GET", f"{base}/folders")
folders = expect_ok(code, folders, "list folders")
if not isinstance(folders, list):
    raise SystemExit(f"list folders: expected array, got {folders}")
child = next((f for f in folders if f.get("name") == "OCC Folder"), None)
if child is None:
    raise SystemExit(f"folder not in list: {folders}")
fid = child["id"]
print(f"created folder id={fid}")

code, note = req("POST", f"{base}/notes", {"folder": fid, "title": "OCC Note", "body": "first"})
note = wait_terminal(code, note, "create note")
if (note.get("body") or {}).get("type") == "error":
    raise SystemExit(f"create note error: {note}")

code, notes = req("GET", f"{base}/notes")
notes = expect_ok(code, notes, "list notes")
if not isinstance(notes, list):
    raise SystemExit(f"list notes: expected array, got {notes}")
got = next((n for n in notes if n.get("title") == "OCC Note"), None)
if got is None:
    raise SystemExit(f"note not in list: {notes}")
nid, rev = got["id"], got["revision"]
print(f"created note id={nid} revision={rev}")

code, saved = req("PUT", f"{base}/notes/{nid}", {"body": "second", "expectedRevision": rev})
saved = wait_terminal(code, saved, "save matching")
if (saved.get("body") or {}).get("type") == "error":
    raise SystemExit(f"matching save should not error: {saved}")
print("matching expectedRevision save ok")

code, listed = req("GET", "/notes/~/v1/notebooks")
listed = expect_ok(code, listed, "list notebooks")
if not isinstance(listed, list):
    raise SystemExit(f"list notebooks: expected array, got {listed}")
if not any(x.get("flagName") == name and x.get("host") == host for x in listed):
    raise SystemExit(f"notebook missing from list: {listed}")
print("list notebooks includes the new notebook")

code, stale = req("PUT", f"{base}/notes/{nid}", {"body": "stale", "expectedRevision": rev})
stale = wait_terminal(code, stale, "save stale")
err = (stale.get("body") or {})
if err.get("type") != "error" or err.get("errorType") != "conflict":
    raise SystemExit(f"stale save should be %conflict, got {stale}")
print("stale expectedRevision save returned %conflict")
print("OCC ok")
PY

echo "files catalog smoke (create, list, stale patch, delete)"
python3 - "$EYRE_URL" "$COOKIES" <<'PY'
import hashlib, json, subprocess, sys, time

eyre, cookie_file = sys.argv[1], sys.argv[2]

def req(method, path, body=None):
    cmd = [
        "curl", "-sS", "-b", cookie_file, "-c", cookie_file,
        "-w", "\n%{http_code}", "--max-time", "60",
        "-X", method, eyre.rstrip("/") + path,
        "-H", "Accept: application/json",
    ]
    if body is not None:
        cmd += ["-H", "Content-Type: application/json", "-d", json.dumps(body)]
    out = subprocess.check_output(cmd)
    raw, _, code_s = out.rpartition(b"\n")
    code = int(code_s.decode())
    return code, (json.loads(raw) if raw else None)

base = "/files/~/v1"
folder = f"/occ-{int(time.time())}"
sha = hashlib.sha256(b"occ").hexdigest()

code, bad = req("POST", f"{base}/files", {"dir": "/a/../b", "name": "x", "sha256": sha, "size": 1})
if code != 400 or bad.get("error") != "bad-request":
    raise SystemExit(f"dotdot dir must be 400 bad-request: HTTP {code} {bad}")
code, bad = req("POST", f"{base}/files", {"dir": folder, "name": "x", "sha256": "ABC", "size": 1})
if code != 400 or bad.get("error") != "bad-request":
    raise SystemExit(f"short sha256 must be 400 bad-request: HTTP {code} {bad}")
print("rejected bad dir and bad sha256 with 400")

code, made = req("POST", f"{base}/files", {
    "dir": folder, "name": "occ.pdf", "sha256": sha, "size": 3, "mime": "application/pdf",
})
if code != 200 or not (made or {}).get("ok"):
    raise SystemExit(f"create file: HTTP {code} {made}")
fid = made["file"]["id"]
print(f"created file id={fid} revision={made['file']['revision']}")

code, listed = req("GET", f"{base}/files")
if code != 200 or not any(f.get("id") == fid for f in (listed or {}).get("files") or []):
    raise SystemExit(f"list files missing {fid}: {listed}")
code, scoped = req("GET", f"{base}/files?dir={folder}")
if code != 200 or [f["id"] for f in scoped.get("files") or []] != [fid]:
    raise SystemExit(f"?dir filter should return only {fid}: {scoped}")
code, dirs = req("GET", f"{base}/dirs")
if code != 200 or folder not in ((dirs or {}).get("dirs") or []):
    raise SystemExit(f"dirs missing {folder}: {dirs}")
print("list files, ?dir filter and dirs include the new entry")

code, moved = req("PUT", f"{base}/files/{fid}", {"name": "occ-renamed.pdf", "expectedRevision": 0})
if code != 200 or not (moved or {}).get("ok") or moved["file"]["revision"] != 1:
    raise SystemExit(f"matching-revision patch: HTTP {code} {moved}")
code, stale = req("PUT", f"{base}/files/{fid}", {"name": "stale.pdf", "expectedRevision": 0})
if code != 409 or stale.get("error") != "conflict":
    raise SystemExit(f"stale patch must be 409 conflict: HTTP {code} {stale}")
print("matching patch ok, stale patch returned 409 conflict")

code, blocked = req("DELETE", f"{base}/dirs?path={folder}")
if code != 409 or blocked.get("error") != "conflict":
    raise SystemExit(f"non-empty folder delete must be 409: HTTP {code} {blocked}")
code, gone = req("DELETE", f"{base}/files/{fid}?expectedRevision=0")
if code != 409:
    raise SystemExit(f"stale delete must be 409: HTTP {code} {gone}")
code, deleted = req("DELETE", f"{base}/files/{fid}?expectedRevision=1")
if code != 200 or not (deleted or {}).get("ok"):
    raise SystemExit(f"delete file: HTTP {code} {deleted}")
code, again = req("DELETE", f"{base}/files/{fid}")
if code != 404:
    raise SystemExit(f"second delete must be 404: HTTP {code} {again}")
code, rmdir = req("DELETE", f"{base}/dirs?path={folder}")
if code != 200 or not (rmdir or {}).get("ok"):
    raise SystemExit(f"empty folder delete: HTTP {code} {rmdir}")
print("stale delete 409, delete 200, repeat delete 404, empty folder delete 200")
print("files smoke ok")
PY

echo "checking +vats / %groups is not running"
# Unauthenticated Eyre 307s /groups to /~/login. After login, a leftover
# %landscape desk 307s to /apps/landscape/. Only a 200 is the groups app.
groups_http="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 8 -b "$COOKIES" "$EYRE_URL/groups" || true)"
if [ "$groups_http" = "200" ]; then
  echo "%groups HTTP surface is up at /groups — Tlon Messenger desk must not be installed" >&2
  exit 1
fi

if sudo test -S "$SOCK"; then
  # %cd lists every Clay desk (the pill still has %groups files). +vats is kiln
  # pikes: fail only when %groups is %live.
  pikes="$(conn_eval '=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(.^(* %gx /(scot %p our)/hood/(scot %da now)/kiln/pikes/noun)))' || true)"
  echo "pikes: $pikes"
  if printf '%s\n' "$pikes" | grep -Eq '%groups[^]]*live'; then
    echo "+vats pike for %groups is %live — Tlon Messenger must not be installed" >&2
    exit 1
  fi
  running="$(conn_eval "=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  now=@da  bind:m  get-time  (pure:m !>(.^((set [@tas ?]) %ge /(scot %p our)/${LOCUS_DESK}/(scot %da now)/$)))" || true)"
  echo "%$LOCUS_DESK agents: $running"
  if ! printf '%s\n' "$running" | grep -q notes; then
    echo "%notes does not appear to be running on %$LOCUS_DESK" >&2
    exit 1
  fi
else
  echo "no conn.sock; skipped kiln pikes. /groups HTTP=$groups_http (not 200)"
fi

echo "done: OCC against our desk, %groups not running"
