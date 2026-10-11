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
# shellcheck source=lib/ship.sh
. "$ROOT/scripts/lib/ship.sh"

need curl
need python3

wait_eyre

commit_desk() {
  echo "uninstalling Tlon %groups / %landscape / pill %notes / pill %contacts if present (this desk is our fork only)"
  conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%groups))  (pure:m !>(%ok))" >/dev/null || true
  conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%landscape))  (pure:m !>(%ok))" >/dev/null || true
  conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%notes))  (pure:m !>(%ok))" >/dev/null || true
  # Gall dude %contacts is on the pill (Tlon). Nuke it so our %contacts is a
  # fresh install, not a revive of negotiate-%2 state.
  if [ "${LOCUS_RESET:-0}" = 1 ]; then conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-nuke+!>([%notes |]))  (pure:m !>(%ok))" >/dev/null || true; fi
  if [ "${LOCUS_RESET:-0}" = 1 ]; then conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-nuke+!>([%contacts |]))  (pure:m !>(%ok))" >/dev/null || true; fi

  echo "creating/merging %$LOCUS_DESK from %base (auto/init)"
  merge_desk_from_base
  wait_desk_exists
  ensure_mounted

  recreate_unix_desk() {
    local mount j
    mount="$(desk_mount)"
    echo "clay unix missed new files; recreating %$LOCUS_DESK mount"
    conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%$LOCUS_DESK))  (pure:m !>(%ok))" >/dev/null || true
    conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-unmount+!>(%$LOCUS_DESK))  (pure:m !>(%ok))" >/dev/null || true
    sleep 2
    sudo rm -rf "$mount"
    merge_desk_from_base
    mount_desk_poke
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
  commit_poke
  wait_committed notes
  if ! clay_has_app locus || ! clay_has_app photos || ! clay_has_app contacts || ! clay_has_app files; then
    recreate_unix_desk
    copy_desk_src
    echo "|commit %$LOCUS_DESK (after recreating mount)"
    hood_poke "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-commit+!>([%$LOCUS_DESK |]))  (pure:m !>(%ok))"
    wait_committed notes
  fi
  wait_committed locus
  wait_committed calendar
  wait_committed photos
  wait_committed contacts
  wait_committed files

  # The nukes above stop %notes / %contacts. If %locus is still installed from a
  # previous run, |install is a no-op and those agents stay dead, so uninstall
  # %locus first (desk files stay in Clay).
  if [ "${LOCUS_RESET:-0}" = 1 ]; then conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-uninstall+!>(%$LOCUS_DESK))  (pure:m !>(%ok))" >/dev/null || true; fi
  sleep 5
  install_poke
  # Do not poke kiln-revive+!>(%locus): this 408k hood nest-fails that vase,
  # bails the khan thread, and can 503 already-running agents. |install plus
  # desk.bill is what starts %notes / %locus after mill succeeds.

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
