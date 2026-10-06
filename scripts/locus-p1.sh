#!/usr/bin/env bash
# P1: household members, agent cannot approve OS mutations, purge cannot
# empty the audit log. Desk must already include %locus (re-run notes-occ.sh
# to |commit). Does not install Tlon Messenger.
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

Commit the desk so desk.bill starts %locus:

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

def req(method, path, body=None, api_key=None):
    cmd = [
        "curl", "-sS",
        "-w", "\n%{http_code}", "--max-time", "30",
        "-X", method, eyre.rstrip("/") + path,
        "-H", "Accept: application/json",
    ]
    if api_key:
        cmd += ["-H", f"X-Api-Key: {api_key}"]
    else:
        cmd += ["-b", cookie_file, "-c", cookie_file]
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

def call(fn, args=None, api_key=None):
    body = {"fn": fn, "args": args or {}}
    code, payload = req("POST", "/locus/~/v1/call", body, api_key=api_key)
    if code != 200:
        raise SystemExit(f"call {fn} HTTP {code}: {payload}")
    return payload

code, members = req("GET", "/locus/~/v1/members")
if code != 200 or not (members or {}).get("ok"):
    raise SystemExit(f"members HTTP {code}: {members}")
if not members.get("members"):
    raise SystemExit(f"expected owner member, got {members}")
print(f"members {len(members['members'])}")

code, me = req("GET", "/locus/~/v1/whoami")
if code != 200 or (me or {}).get("role") != "owner":
    raise SystemExit(f"owner whoami: {me}")
print(f"owner {me.get('ship')}")

added = call("household.add", {"ship": "~dev", "name": "worker", "role": "agent"})
if added.get("ok"):
    agent_key = added.get("apiKey")
    print("added agent ~dev")
elif added.get("error") == "exists":
    issued = call("household.issue-key", {"ship": "~dev"})
    if not issued.get("ok") or not issued.get("apiKey"):
        raise SystemExit(f"issue-key: {issued}")
    agent_key = issued["apiKey"]
    print("rotated key for existing agent ~dev")
else:
    raise SystemExit(f"add agent: {added}")
if not agent_key:
    raise SystemExit("no agent apiKey")

code, agent = req("GET", "/locus/~/v1/whoami", api_key=agent_key)
if code != 200 or (agent or {}).get("ship") != "~dev" or (agent or {}).get("role") != "agent":
    raise SystemExit(f"agent whoami: {agent}")
print("agent authenticated as ~dev")

queued = call("os.rebuild", {"flake": ".#server"}, api_key=agent_key)
if not queued.get("ok") or not queued.get("job"):
    raise SystemExit(f"agent rebuild should queue: {queued}")
jid = queued["job"]["id"]
print(f"queued os.rebuild id={jid}")

denied = call("os.approve", {"id": jid}, api_key=agent_key)
if denied.get("ok"):
    raise SystemExit(f"agent must not approve: {denied}")
print("agent approve rejected")

ok = call("os.approve", {"id": jid})
if not ok.get("ok"):
    raise SystemExit(f"owner approve failed: {ok}")
print("owner approved")

purge = call("history.purge", {})
if purge.get("ok"):
    raise SystemExit(f"purge must be denied: {purge}")

code, audit = req("GET", "/locus/~/v1/audit")
if code != 200 or not (audit or {}).get("audit"):
    raise SystemExit(f"audit empty after purge attempt: {audit}")
print(f"audit {len(audit['audit'])} entries (purge did not wipe)")
print("P1 locus check passed")
PY
