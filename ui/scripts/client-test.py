#!/usr/bin/env python3
"""A client ship with its own host, against a home ship with a real-sized store.
Checks replication through both hosts, then reactions, revert and purge.
Usage: client-test.py <home host url> <client host url>"""
import json, sys, time, random, urllib.request as u
HOME, CLIENT = sys.argv[1:3]
fails = 0
def req(url, body=None):
    d = None if body is None else json.dumps(body).encode()
    r = u.Request(url, d, {'content-type': 'application/json'})
    return json.loads(u.urlopen(r, timeout=30).read() or 'null')
state = lambda h: req(f'{h}/store/~/v1/state')
do = lambda h, b: req(f'{h}/store/~/v1/do', b)
def obj(h, i):
    return next((o for o in state(h)['objects'] if o['id'] == i), None)
def check(name, f, wait=40):
    global fails
    for _ in range(wait):
        try:
            if f(): print('PASS', name); return True
        except Exception: pass
        time.sleep(1)
    fails += 1; print('FAIL', name); return False
batch = lambda ops, actor=None: {'do': 'batch', 'cid': f'c{random.random()}', 'intent': 'test', 'ops': ops}
R = str(random.randrange(10**8))

check('client holds the whole store', lambda: len(state(CLIENT)['objects']) >= len(state(HOME)['objects']), 90)
a = 'a' + R
do(CLIENT, batch([{'do': 'make', 'id': a, 'type': 'note', 'fields': {'title': 'from client'}}]))
check('client write shows at once on the client', lambda: obj(CLIENT, a), 3)
check('client write reaches home', lambda: obj(HOME, a))

# Revert: undo a title change from the log.
do(HOME, batch([{'do': 'set', 'id': a, 'base': 0, 'fields': {'title': 'changed'}}]))
check('set applies', lambda: obj(HOME, a)['fields']['title'] == 'changed', 5)
seq = state(HOME)['seq']
do(HOME, {'do': 'revert', 'seq': seq})
check('revert restores the title', lambda: obj(HOME, a)['fields']['title'] == 'from client', 5)
check('revert reaches client', lambda: obj(CLIENT, a)['fields']['title'] == 'from client')

# Reaction: a view over notes tagged R; when it changes the tool stamps them.
tool, view = 'tool' + R, 'view' + R
prog = [{'for': {'var': 'view'}, 'as': 'x', 'do': [{'set': {'id': {'var': 'x'}, 'fields': {'obj': {'seen': 'yes'}}}}]}]
do(HOME, batch([
    {'do': 'make', 'id': tool, 'type': 'tool', 'fields': {'name': 'stamp', 'program': prog, 'can': ['set'], 'examples': []}},
    {'do': 'make', 'id': f'grant:{tool}', 'type': 'grant', 'fields': {'actor': f'tool:{tool}', 'ops': ['set'], 'types': ['note']}},
    {'do': 'make', 'id': view, 'type': 'view', 'fields': {'query': {'type': 'note', 'where': [{'field': 'tag', 'is': R}]}, 'react': tool}},
]))
b = 'b' + R
do(HOME, batch([{'do': 'make', 'id': b, 'type': 'note', 'fields': {'title': 'x', 'tag': R}}]))
check('granted reaction applies', lambda: obj(HOME, b)['fields'].get('seen') == 'yes', 8)
do(HOME, {'do': 'batch', 'cid': f'c{R}', 'intent': 'test', 'ops': [{'do': 'trash', 'id': f'grant:{tool}'}]})
c = 'c' + R
do(HOME, batch([{'do': 'make', 'id': c, 'type': 'note', 'fields': {'title': 'y', 'tag': R}}]))
check('ungranted reaction proposes instead', lambda: any(o['type'] == 'proposal' and f'tool:{tool}' == o['fields'].get('actor') and o['fields'].get('status') == 'open' for o in state(HOME)['objects']), 8)
check('and writes nothing', lambda: 'seen' not in obj(HOME, c)['fields'], 2)

# Purge: the value is gone from object and log, on both ships.
secret = 'hunter' + R
d = 'd' + R
do(HOME, batch([{'do': 'make', 'id': d, 'type': 'note', 'fields': {'title': secret}}]))
check('secret arrives', lambda: obj(CLIENT, d))
do(HOME, batch([{'do': 'purge', 'id': d}]))
log = lambda h: json.dumps(req(f'{h}/store/~/v1/log?since=0')) + json.dumps(state(h))
check('purge scrubs home', lambda: secret not in log(HOME), 8)
check('purge scrubs client', lambda: secret not in log(CLIENT))
sys.exit(1 if fails else 0)
