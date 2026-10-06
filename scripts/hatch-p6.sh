#!/usr/bin/env bash
# P6: queue escape.launch for steam/blender on %locus. Does not launch the
# binaries (no LOCUS_HATCH_EXECUTE). Desk must already include %locus.
#
# Env: EYRE_URL (default http://127.0.0.1), NOTES_CODE_FILE, or urbit-code.
set -euo pipefail

EYRE_URL="${EYRE_URL:-http://127.0.0.1}"

need() { command -v "$1" >/dev/null 2>&1 || { echo "need $1" >&2; exit 1; }; }
need curl
need python3

spec="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 8 "$EYRE_URL/locus/openapi.json" || true)"
if [ "$spec" != "200" ]; then
  cat >&2 <<EOF
%locus is not serving $EYRE_URL/locus/openapi.json (HTTP $spec).

Commit the desk:

  EYRE_URL=$EYRE_URL ./scripts/notes-occ.sh

Then re-run this script. +code is never printed.
EOF
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

def call(fn, args=None):
    body = {"fn": fn, "args": args or {}}
    code, payload = req("POST", "/locus/~/v1/call", body)
    if code != 200:
        raise SystemExit(f"call {fn} HTTP {code}: {payload}")
    return payload

listed = call("escape.list", {})
if not listed.get("ok"):
    raise SystemExit(f"escape.list: {listed}")
apps = listed.get("apps") or []
if "steam" not in apps or "blender" not in apps:
    raise SystemExit(f"escape.list want steam+blender, got {apps}")
print(f"escape.list {apps}")

denied = call("escape.launch", {"app": "doom"})
if denied.get("ok"):
    raise SystemExit(f"unknown hatch app must not queue: {denied}")
print("unknown hatch app rejected")

queued = call("escape.launch", {"app": "steam"})
if not queued.get("ok") or not queued.get("job"):
    raise SystemExit(f"escape.launch steam should queue: {queued}")
jid = queued["job"]["id"]
print(f"queued escape.launch steam id={jid}")

code, pending = req("GET", "/locus/~/v1/pending")
if code != 200 or not any(j.get("id") == jid for j in (pending or {}).get("pending") or []):
    raise SystemExit(f"pending missing {jid}: {pending}")
print("pending includes the hatch job (not launched)")
print("P6 hatch check passed")
PY
