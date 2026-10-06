#!/usr/bin/env bash
# P4: photo catalog + contacts on desk %locus. Create/list/expectedRevision
# conflict; reject `..` catalog paths. Bytes stay on ZFS — this script does
# not copy JPEGs. Desk must include %photos / %contacts (re-run notes-occ.sh).
#
# Env: EYRE_URL (default http://127.0.0.1), NOTES_CODE_FILE, or urbit-code.
set -euo pipefail

EYRE_URL="${EYRE_URL:-http://127.0.0.1}"

need() { command -v "$1" >/dev/null 2>&1 || { echo "need $1" >&2; exit 1; }; }
need curl
need python3

wait_spec() {
  local path=$1 name=$2
  local i
  for i in $(seq 1 90); do
    if [ "$(curl -sS -o /dev/null -w '%{http_code}' --max-time 5 "$EYRE_URL$path" || true)" = "200" ]; then
      echo "$name is serving $path"
      return 0
    fi
    sleep 2
  done
  return 1
}

if ! wait_spec /photos/openapi.json %photos; then
  cat >&2 <<EOF
%photos is not serving $EYRE_URL/photos/openapi.json.

Commit the desk so desk.bill starts %photos and %contacts:

  EYRE_URL=$EYRE_URL ./scripts/notes-occ.sh

Then re-run this script. +code is never printed.
EOF
  journalctl -u urbit -n 80 --no-pager >&2 || true
  exit 2
fi
if ! wait_spec /contacts/openapi.json %contacts; then
  echo "%contacts did not come up" >&2
  journalctl -u urbit -n 80 --no-pager >&2 || true
  exit 2
fi

CODE_FILE="$(mktemp)"
COOKIES="$(mktemp)"
cleanup() { rm -f "$CODE_FILE" "$COOKIES"; }
trap cleanup EXIT
chmod 600 "$CODE_FILE"

if [ -n "${NOTES_CODE_FILE:-}" ]; then
  tr -d '\n\r' <"$NOTES_CODE_FILE" >"$CODE_FILE"
elif command -v urbit-code >/dev/null 2>&1; then
  sudo -u urbit urbit-code | tr -d '\n\r' >"$CODE_FILE"
else
  echo "cannot get +code: run on the server (sudo -u urbit urbit-code) or set NOTES_CODE_FILE" >&2
  exit 1
fi
if [ ! -s "$CODE_FILE" ]; then
  echo "empty +code" >&2
  exit 1
fi

login_code="$(curl -sS -o /dev/null -w '%{http_code}' -c "$COOKIES" \
  -X POST "$EYRE_URL/~/login" \
  --data-urlencode "password@$CODE_FILE")"
: >"$CODE_FILE"
if [ "$login_code" != "204" ] && ! grep -q urbauth "$COOKIES"; then
  echo "login failed (HTTP $login_code)" >&2
  exit 1
fi
echo "logged in to Eyre"

python3 - "$EYRE_URL" "$COOKIES" <<'PY'
import json, subprocess, sys

eyre, cookie_file = sys.argv[1], sys.argv[2]

def req(method, path, body=None):
    cmd = [
        "curl", "-sS",
        "-w", "\n%{http_code}", "--max-time", "30",
        "-X", method, eyre.rstrip("/") + path,
        "-H", "Accept: application/json",
        "-b", cookie_file, "-c", cookie_file,
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

code, bad = req("POST", "/photos/~/v1/photos", {
    "album": "x", "title": "x", "path": "../etc/passwd",
})
if code != 200 or (bad or {}).get("ok"):
    raise SystemExit(f"dotdot path must be rejected: HTTP {code} {bad}")
print("rejected .. catalog path")

code, created = req("POST", "/photos/~/v1/photos", {
    "album": "holidays",
    "title": "pier",
    "path": "holidays/pier.jpg",
    "tags": ["sea"],
})
if code != 200 or not (created or {}).get("ok") or not (created or {}).get("photo"):
    raise SystemExit(f"create photo: HTTP {code} {created}")
pid = created["photo"]["id"]
prev = created["photo"]["revision"]
print(f"created photo id={pid} rev={prev}")

code, listed = req("GET", "/photos/~/v1/photos")
if code != 200 or not any(p.get("id") == pid for p in (listed or {}).get("photos") or []):
    raise SystemExit(f"list photos missing {pid}: {listed}")
print("list photos includes the new entry")

code, saved = req("PUT", f"/photos/~/v1/photos/{pid}", {
    "title": "ok", "expectedRevision": prev,
})
if code != 200 or not (saved or {}).get("ok"):
    raise SystemExit(f"matching photo revision: {saved}")
print("matching expectedRevision photo save ok")

code, stale = req("PUT", f"/photos/~/v1/photos/{pid}", {
    "title": "stale", "expectedRevision": prev,
})
if code != 200 or (stale or {}).get("error") != "conflict":
    raise SystemExit(f"stale photo should be conflict, got {stale}")
print("stale expectedRevision photo save returned conflict")

code, contact = req("POST", "/contacts/~/v1/contacts", {
    "name": "Ada", "email": "ada@home", "ship": "~zod",
})
if code != 200 or not (contact or {}).get("ok") or not (contact or {}).get("contact"):
    raise SystemExit(f"create contact: HTTP {code} {contact}")
cid = contact["contact"]["id"]
crev = contact["contact"]["revision"]
print(f"created contact id={cid} rev={crev}")

code, clist = req("GET", "/contacts/~/v1/contacts")
if code != 200 or not any(c.get("id") == cid for c in (clist or {}).get("contacts") or []):
    raise SystemExit(f"list contacts missing {cid}: {clist}")
print("list contacts includes the new contact")

code, csaved = req("PUT", f"/contacts/~/v1/contacts/{cid}", {
    "name": "ok", "expectedRevision": crev,
})
if code != 200 or not (csaved or {}).get("ok"):
    raise SystemExit(f"matching contact revision: {csaved}")

code, cstale = req("PUT", f"/contacts/~/v1/contacts/{cid}", {
    "name": "stale", "expectedRevision": crev,
})
if code != 200 or (cstale or {}).get("error") != "conflict":
    raise SystemExit(f"stale contact should be conflict, got {cstale}")
print("stale expectedRevision contact save returned conflict")
print("P4 photos/contacts check passed")
PY
