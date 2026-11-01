#!/usr/bin/env bash
# %store replication on two FAKE ships (never the real one): ~zod is home, ~nec is a client.
# Needs ships at /tmp/fz/zod (HTTP 8081) and /tmp/fz/nec (HTTP 8082) with %locus installed.
set -uo pipefail
C=/tmp/fz; FAILS=0
pass() { echo "PASS $*"; }; fail() { echo "FAIL $*"; FAILS=$((FAILS+1)); }
code() { printf '%s\n' "[0 %fyrd %base %khan-eval %noun %ted-eval '=/  m  (strand ,vase)  ;<  our=@p  bind:m  get-our  ;<  code=@p  bind:m  (scry @p /j/code/(scot %p our))  (pure:m !>((crip (slag 1 (scow %p code)))))']" | urbit eval -jn 2>/dev/null | nc -U -W 5 "$1/.urb/conn.sock" | urbit eval -cn 2>/dev/null | grep -Eo '[a-z]{3,8}(-[a-z]{3,8}){3}' | head -1; }
login() { curl -s -o /dev/null -c "$C/c$1" -X POST --data-urlencode "password=$(code "$2")" "http://localhost:$1/~/login"; }
do_() { curl -s -b "$C/c$1" -H 'content-type: application/json' --data "$2" "http://localhost:$1/store/~/v1/do"; }
st() { curl -s -b "$C/c$1" "http://localhost:$1/store/~/v1/state"; }
get() { st "$1" | python3 -c "
import json,sys
d=json.load(sys.stdin); m=[o for o in d['objects'] if o['id']=='$2']
print(m[0]['fields'].get('$3') if m else 'missing')"; }
pend() { st "$1" | python3 -c "import json,sys; print(json.load(sys.stdin)['pending'])"; }
wait_for() { local want=$1; shift; for i in $(seq 1 ${WAIT:-40}); do [ "$("$@" 2>/dev/null)" = "$want" ] && return 0; sleep 1; done; return 1; }
check() { local name=$1 want=$2; shift 2; if wait_for "$want" "$@"; then pass "$name"; else fail "$name (want $want, got $("$@" 2>/dev/null))"; fi; }
mk() { do_ "$1" "{\"do\":\"batch\",\"cid\":\"$2\",\"intent\":\"t\",\"ops\":[{\"do\":\"make\",\"id\":\"$2\",\"type\":\"note\",\"fields\":{\"title\":\"$3\"}}]}" >/dev/null; }
set_() { do_ "$1" "{\"do\":\"batch\",\"cid\":\"$2-$RANDOM\",\"intent\":\"t\",\"ops\":[{\"do\":\"set\",\"id\":\"$2\",\"base\":$4,\"fields\":{\"title\":\"$3\"}}]}" >/dev/null; }
stop_ship() { systemctl --user stop fz-$1; sleep 3; }
start_ship() { systemctl --user start fz-$1-run 2>/dev/null; systemd-run --user --unit=fz-$1-run --working-directory=/tmp/fz bash -c "exec script -qfc 'urbit -t --http-port $2 -p $3 --loom 31 $1' /tmp/fz/$1.run" >/dev/null 2>&1; for i in $(seq 1 120); do curl -s -o /dev/null --max-time 2 localhost:$2/~/login && break; sleep 1; done; }
R=$RANDOM; a=a$R; b=b$R; c=c$R; d=d$R; e=e$R
login 8081 $C/zod; login 8082 $C/nec
mk 8081 $a "from home"
do_ 8081 '{"do":"member","ship":"~nec"}' >/dev/null
do_ 8082 '{"do":"follow","ship":"~zod"}' >/dev/null
check "client gets a snapshot of home" "from home" get 8082 $a title
mk 8082 $b "from client"
[ "$(get 8082 $b title)" = "from client" ] && pass "a client write shows at once" || fail "client write not immediate"
check "home accepts the client write" "from client" get 8081 $b title
check "client's pending clears" 0 pend 8082
mk 8081 $c "live"
check "home writes reach the client" "live" get 8082 $c title
echo "-- client offline"
systemctl --user stop fz-nec fz-nec-run 2>/dev/null; sleep 4
mk 8081 $d "while client away"
start_ship nec 8082 31338; login 8082 $C/nec
check "client catches up after being away" "while client away" get 8082 $d title
echo "-- home offline"
systemctl --user stop fz-zod fz-zod-run 2>/dev/null; sleep 4
mk 8082 $e "written offline"
[ "$(get 8082 $e title)" = "written offline" ] && pass "client keeps working with home down" || fail "offline write"
[ "$(pend 8082)" = 1 ] && pass "offline write is provisional" || fail "pending $(pend 8082)"
start_ship zod 8081 31337; login 8081 $C/zod
WAIT=180 check "offline write reaches home" "written offline" get 8081 $e title
WAIT=60 check "and is confirmed" 0 pend 8082
echo "-- conflict"
set_ 8081 $a "home edit" 1
set_ 8082 $a "client edit" 1
check "first in home's order wins" "home edit" get 8081 $a title
check "the other write is kept as a clash" 1 bash -c "curl -s -b $C/c8081 localhost:8081/store/~/v1/state | python3 -c \"import json,sys; print(len([o for o in json.load(sys.stdin)['objects'] if o['id']=='$a'][0]['clashes']))\""
check "client converges" "home edit" get 8082 $a title
echo "failures: $FAILS"; exit $FAILS
