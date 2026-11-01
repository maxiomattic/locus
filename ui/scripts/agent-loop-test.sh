#!/usr/bin/env bash
# The learning loop end to end on a FAKE ship with a stub model. Needs a host on $1 (default 5281).
U=http://127.0.0.1:${1:-5281}; FAILS=0
post() { curl -s -H 'content-type: application/json' --data "$2" "$U$1"; }
stat() { curl -s "$U/agent/stats" | python3 -c "import json,sys; print(json.load(sys.stdin)['calls'])"; }
objs() { curl -s "$U/store/~/v1/state" | python3 -c "import json,sys
d=json.load(sys.stdin); print(len([o for o in d['objects'] if $1]))"; }
pick() { curl -s "$U/store/~/v1/state" | python3 -c "import json,sys
d=json.load(sys.stdin); print([o['id'] for o in d['objects'] if $1][-1])"; }
rot() { curl -s "$U/store/~/v1/state" | python3 -c "import json,sys
d=json.load(sys.stdin); print([o['fields'].get('rot') for o in d['objects'] if o['id']=='$1'][0])"; }
ok() { if [ "$2" = "$3" ]; then echo "PASS $1"; else echo "FAIL $1 (want $3, got $2)"; FAILS=$((FAILS+1)); fi; }
PH=($(curl -s "$U/store/~/v1/state" | python3 -c "import json,sys
print(' '.join(sorted(o['id'] for o in json.load(sys.stdin)['objects'] if o['type']=='photo')[:4]))"))
c0=$(stat); p0=$(objs "o['type']=='proposal'")
r=$(post /agent/ask "{\"text\":\"rotate this photo\",\"selection\":[\"${PH[0]}\"]}"); echo "$r"
ok "first request reaches the model" $(( $(stat)-c0 )) 1
ok "a change is only a proposal" "$(rot ${PH[0]})" 0
ok "one proposal" $(( $(objs "o['type']=='proposal'")-p0 )) 1
r=$(post /agent/ask "{\"text\":\"turn and rotate that picture\",\"selection\":[\"${PH[1]}\"]}"); echo "$r"
ok "second request, other words, same plan: offers a tool" "$(echo "$r" | python3 -c "import json,sys; print(json.load(sys.stdin).get('learned'))")" True
tp=$(pick "o['type']=='proposal' and o['fields']['actor']=='agent:host' and o['fields']['status']=='open'")
post /store/~/v1/do "{\"do\":\"approve\",\"id\":\"$tp\"}" >/dev/null
ok "approving makes the tool" "$(objs "o['type']=='tool'")" 1
T=$(pick "o['type']=='tool'")
c1=$(stat)
post /agent/tool "{\"id\":\"$T\",\"selection\":[\"${PH[2]}\"]}"; echo
ok "the key runs with no model" $(( $(stat)-c1 )) 0
ok "it did the work" "$(rot ${PH[2]})" 90
post /agent/tool "{\"id\":\"$T\",\"selection\":[\"${PH[2]}\"]}" >/dev/null
ok "it computes from current state" "$(rot ${PH[2]})" 180
r=$(post /agent/ask "{\"text\":\"rotate this photo\",\"selection\":[\"${PH[3]}\"]}"); echo "$r"
ok "asking again uses the tool" $(( $(stat)-c1 )) 0
ok "and did the work" "$(rot ${PH[3]})" 90
echo "failures: $FAILS"; exit $FAILS
