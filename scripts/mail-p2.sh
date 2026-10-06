#!/usr/bin/env bash
# P2: mail + calendar Gall stores. Create/list/expectedRevision conflict,
# ingest from a fixture .eml. Desk must include %mail and %calendar
# (re-run notes-occ.sh to |commit). Does not install Tlon Messenger.
#
# Env: EYRE_URL (default http://127.0.0.1), LOCUS_DESK (default locus),
# NOTES_CODE_FILE, or urbit-code.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EYRE_URL="${EYRE_URL:-http://127.0.0.1}"
URBIT_PIER="${URBIT_PIER:-/var/lib/urbit/pier/ship}"
LOCUS_DESK="${LOCUS_DESK:-locus}"
SOCK="$URBIT_PIER/.urb/conn.sock"
FIXTURE="$ROOT/scripts/fixtures/p2-ingest.eml"

need() { command -v "$1" >/dev/null 2>&1 || { echo "need $1" >&2; exit 1; }; }
need curl
need python3

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

if [ "$(curl -sS -o /dev/null -w '%{http_code}' --max-time 8 "$EYRE_URL/mail/openapi.json" || true)" != "200" ]; then
  if sudo test -S "$SOCK" && urbit_bin >/dev/null 2>&1; then
    echo "|revive %$LOCUS_DESK (start %mail / %calendar from desk.bill)"
    conn_eval "=/  m  (strand ,vase)  ;<  ~  bind:m  (poke-our %hood kiln-revive+!>(%$LOCUS_DESK))  (pure:m !>(%ok))" >/dev/null || true
  fi
fi

if ! wait_spec /mail/openapi.json %mail; then
  cat >&2 <<EOF
%mail is not serving $EYRE_URL/mail/openapi.json.

Commit the desk so desk.bill starts %mail and %calendar:

  EYRE_URL=$EYRE_URL ./scripts/notes-occ.sh

Then re-run this script. +code is never printed.
EOF
  journalctl -u urbit -n 80 --no-pager >&2 || true
  exit 2
fi
if ! wait_spec /calendar/openapi.json %calendar; then
  echo "%calendar did not come up" >&2
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

python3 - "$EYRE_URL" "$COOKIES" "$FIXTURE" <<'PY'
import json, subprocess, sys
from email.parser import Parser

eyre, cookie_file, fixture = sys.argv[1], sys.argv[2], sys.argv[3]

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

code, created = req("POST", "/mail/~/v1/messages", {"subject": "hello", "body": "draft body"})
if code != 200 or not (created or {}).get("ok") or not (created or {}).get("msg"):
    raise SystemExit(f"create draft: HTTP {code} {created}")
mid = created["msg"]["id"]
rev = created["msg"]["revision"]
print(f"created draft id={mid} rev={rev}")

code, listed = req("GET", "/mail/~/v1/messages?folder=drafts")
if code != 200 or not (listed or {}).get("ok"):
    raise SystemExit(f"list drafts: {listed}")
ids = [m["id"] for m in listed.get("messages") or []]
if mid not in ids:
    raise SystemExit(f"draft {mid} missing from {ids}")
print("list drafts includes the new message")

code, saved = req("PUT", f"/mail/~/v1/messages/{mid}", {"body": "ok", "expectedRevision": rev})
if code != 200 or not (saved or {}).get("ok"):
    raise SystemExit(f"save matching revision: {saved}")
print("matching expectedRevision save ok")

code, stale = req("PUT", f"/mail/~/v1/messages/{mid}", {"body": "stale", "expectedRevision": rev})
if code != 200 or (stale or {}).get("error") != "conflict":
    raise SystemExit(f"stale save should be conflict, got {stale}")
print("stale expectedRevision save returned conflict")

raw = open(fixture, encoding="utf-8").read()
parsed = Parser().parsestr(raw)
ingested = {
    "from": parsed.get("From", ""),
    "to": parsed.get("To", ""),
    "subject": parsed.get("Subject", ""),
    "body": (parsed.get_payload() or "").strip(),
}
code, ing = req("POST", "/mail/~/v1/ingest", ingested)
if code != 200 or not (ing or {}).get("ok"):
    raise SystemExit(f"ingest: {ing}")
if (ing.get("msg") or {}).get("folder") != "inbox":
    raise SystemExit(f"ingest should land in inbox: {ing}")
print(f"ingested fixture into inbox id={ing['msg']['id']}")

code, ev = req("POST", "/calendar/~/v1/events", {
    "title": "standup",
    "start": 1700000000,
    "end": 1700003600,
})
if code != 200 or not (ev or {}).get("ok") or not (ev or {}).get("event"):
    raise SystemExit(f"create event: HTTP {code} {ev}")
eid = ev["event"]["id"]
erev = ev["event"]["revision"]
print(f"created event id={eid} rev={erev}")

code, elist = req("GET", "/calendar/~/v1/events")
if code != 200 or not any(e.get("id") == eid for e in (elist or {}).get("events") or []):
    raise SystemExit(f"list events missing {eid}: {elist}")
print("list events includes the new event")

code, esaved = req("PUT", f"/calendar/~/v1/events/{eid}", {"title": "ok", "expectedRevision": erev})
if code != 200 or not (esaved or {}).get("ok"):
    raise SystemExit(f"event matching revision: {esaved}")

code, estale = req("PUT", f"/calendar/~/v1/events/{eid}", {"title": "stale", "expectedRevision": erev})
if code != 200 or (estale or {}).get("error") != "conflict":
    raise SystemExit(f"stale event should be conflict, got {estale}")
print("stale event expectedRevision save returned conflict")
print("P2 mail/calendar check passed")
PY
