import test from 'node:test'
import assert from 'node:assert/strict'
import { init, press, frame, items, titleOf, viewActions, applyRemote, validPeer } from '../src/engine.js'
import { diff } from '../src/sync.js'
import { createAgent } from '../server/agent.mjs'
import { createMessages, fromDa, messageSetup } from '../server/messages.mjs'
import { createShip } from '../demo/ship.js'
import { fakeMessages } from '../demo/messages.js'

const KEY = { sp: ' ', esc: 'Escape', ent: 'Enter' }
function type(s, seq) {
  for (const t of seq.split(' ')) {
    if (t === '') continue
    if (KEY[t]) s = press(s, { key: KEY[t] })
    else for (const c of t) s = press(s, { key: c, shift: c !== c.toLowerCase() })
  }
  return s
}
const words = (s, text) => { for (const c of text) s = press(s, { key: c }); return s }
const thread = (s) => Object.values(s.store).find((o) => o.type === 'thread')
const inThread = () => type(init(), 'M sp')
const draft = (s) => Object.values(s.store).find((o) => o.status === 'draft')

test('M opens Messages from anywhere, and Space opens a thread at its newest message', () => {
  let s = type(init(), 'p M')
  assert.equal(titleOf(s, frame(s).id), 'Messages')
  s = type(s, 'sp')
  assert.equal(frame(s).focus, thread(s).items.length - 1)
  s = type(s, 'k k')
  assert.equal(frame(s).focus, 0)
})

test('Enter stops typing and keeps the draft. It never sends', () => {
  let s = type(inThread(), 'o')
  assert.equal(s.mode, 'typing')
  s = words(s, 'hello there')
  s = type(s, 'ent')
  assert.equal(s.mode, 'normal')
  assert.equal(draft(s).text, 'hello there')
  assert.deepEqual(s.out, [])
  assert.ok(items(s, frame(s)).includes(draft(s).id))
})

test('A sends, and the send is not an edit', () => {
  let s = words(type(inThread(), 'o'), 'hi')
  s = type(s, 'esc')
  assert.ok(viewActions(s).some(([k, l]) => k === 'A' && l === 'Send'))
  const id = draft(s).id
  const undos = s.undo.length
  s = type(s, 'A')
  assert.deepEqual(s.out, [{ do: 'send', id }])
  assert.equal(s.undo.length, undos)
  assert.equal(diff(s.undo.at(-1).store, s.store).filter((op) => op.fields?.status && op.do === 'set').length, 0)
  assert.ok(!viewActions(s).some(([k]) => k === 'A'), 'a message on its way can’t be sent twice')
})

test('there is nothing to send without a draft, and an empty draft goes away', () => {
  let s = inThread()
  assert.ok(!viewActions(s).some(([k]) => k === 'A'))
  s = type(s, 'A')
  assert.deepEqual(s.out, [])
  const n = thread(s).items.length
  s = type(s, 'o esc')
  assert.equal(thread(s).items.length, n)
  assert.equal(draft(s), undefined)
})

test('undo can’t reach a sent message, or one that arrived', () => {
  let s = words(type(inThread(), 'o'), 'gone')
  s = type(s, 'esc')
  const id = draft(s).id
  const t = thread(s).id
  s = type(s, 'A')
  s = type(s, 'z')
  assert.equal(s.store[id].text, 'gone')
  assert.ok(s.store[t].items.includes(id))
  s = applyRemote(s, [
    { ...s.store[id], status: 'delivered', $sending: undefined },
    { id: 'in', type: 'message', text: 'yo', from: '~nec', status: 'received' },
    { ...s.store[t], items: [...s.store[t].items, 'in'] },
  ])
  s = type(s, 'z Z z')
  assert.equal(s.store[id].status, 'delivered')
  assert.ok(s.store[t].items.includes('in'))
  assert.ok(s.store.in)
})

test('undo still takes back a draft that hasn’t gone', () => {
  let s = words(type(inThread(), 'o'), 'maybe')
  s = type(s, 'esc z')
  assert.equal(draft(s), undefined)
})

test('a sent message can be copied but not cut, and pastes into a note as a link back', () => {
  let s = inThread()
  const n = thread(s).items.length
  s = type(s, 'x')
  assert.match(s.msg, /unsent/)
  assert.equal(thread(s).items.length, n)
  s = type(s, 'c n sp')
  s = type(s, 'G v')
  const note = s.store[frame(s).id]
  const b = note.items.map((x) => s.store[x]).find((x) => s.store[x.ref]?.type === 'message')
  assert.ok(b)
  assert.equal(frame(s).focus, note.items.indexOf(b.id))
  s = type(s, 'sp')
  assert.equal(frame(s).id, thread(s).id)
})

test('pasting an object into a thread adds its name to the draft', () => {
  const s = type(init(), 'e c M sp v')
  assert.equal(draft(s).text, 'Lauren Okafor')
  assert.match(s.msg, /text only/)
})

test('a failed message offers A again, under the cursor', () => {
  let s = inThread()
  const last = thread(s).items.at(-1)
  s = applyRemote(s, [{ ...s.store[last], status: 'failed' }])
  assert.ok(viewActions(s).some(([k, l]) => k === 'A' && l === 'Send again'))
  s = type(s, 'A')
  assert.deepEqual(s.out, [{ do: 'send', id: last }])
})

test('a new thread asks for its ship, refuses a bad one, and can’t send without one', () => {
  let s = type(init(), 'M o')
  s = words(s, 'Sam')
  s = type(s, 'ent')
  assert.equal(s.typing.field, 'peer')
  s = words(s, 'not a ship')
  s = type(s, 'ent')
  assert.equal(s.mode, 'typing')
  assert.match(s.msg, /ship name/)
  s = type(s, 'esc')
  const t = Object.values(s.store).find((o) => o.title === 'Sam')
  s = { ...s, store: { ...s.store, [t.id]: { ...s.store[t.id], peer: '' } } }
  s = type(s, 'sp o')
  s = words(s, 'hi')
  s = type(s, 'esc A')
  assert.deepEqual(s.out, [])
  assert.match(s.msg, /no ship/)
  s = type(s, 'P')
  s = words(s, 'sampel-palnet')
  s = type(s, 'ent')
  assert.equal(s.store[t.id].peer, '~sampel-palnet')
  s = type(s, 'A')
  assert.equal(s.out[0].do, 'send')
})

test('ship names', () => {
  for (const p of ['~zod', '~marzod', '~sampel-palnet', '~doznec-marzod-sampel-palnet', '~masned-savted-difted-locden--rovruc-sonsem-witper-wanzod']) assert.ok(validPeer(p), p)
  for (const p of ['', 'zod', '~Zod', '~ab', '~a b', '~zod-', undefined]) assert.ok(!validPeer(p), p)
})

test('@da becomes seconds', () => assert.equal(fromDa('~2026.11.2..10.00.05..abcd'), Date.UTC(2026, 10, 2, 10, 0, 5) / 1000))

// ---------- The bridge, against the store's rules and a stub %messages ----------

function rig() {
  const ship = createShip()
  const kind = (fields) => ({ do: 'make', id: `type:${fields.name}`, type: 'type', fields })
  ship.load([
    ...messageSetup(kind),
    { do: 'make', id: 'messages', type: 'collection', fields: { title: 'Messages', layout: 'list', items: ['t1'] } },
    { do: 'make', id: 'd1', type: 'message', fields: { text: 'hello', from: 'me', status: 'draft' } },
    { do: 'make', id: 't1', type: 'thread', fields: { title: 'Nec', peer: '~nec', items: ['d1'] } },
    { do: 'make', id: 'grant:runs', type: 'grant', fields: { actor: 'agent:host', ops: ['make', 'set'], types: ['run'] } },
  ])
  const call = async (method, url, body) => {
    const [code, data] = ship.http(method, `/store/~/v1${url}`, body)
    if (code !== 200) throw new Error(data.error)
    return data
  }
  const net = { all: [], sent: [], id: 0, fail: null }
  const api = async (method, url, body) => {
    if (method === 'POST' && url === '/messages') {
      if (net.fail) throw new Error(net.fail)
      net.sent.push(body)
      const m = { id: ++net.id, peer: body.to, outgoing: true, body: body.body, status: 'pending', sent: '~2026.11.2..10.00.00', group: null }
      net.all.push(m)
      return { ok: true, id: m.id }
    }
    if (url === '/conversations') {
      const peers = [...new Set(net.all.map((m) => m.peer))]
      return { conversations: peers.map((peer) => { const l = net.all.filter((m) => m.peer === peer); return { peer, count: l.length, unread: l.filter((m) => !m.outgoing).length, last: l.at(-1) } }) }
    }
    if (url.startsWith('/conversations/')) return { messages: net.all.filter((m) => m.peer === url.split('/')[2]) }
    return { ok: true }
  }
  const get = (id) => ship.data.objects[id]
  return { ship, call, net, api, get, bridge: createMessages({ call, api, log: () => {} }) }
}

test('send delivers what the store holds, and only once', async () => {
  const r = rig()
  const [a, b] = await Promise.all([r.bridge.send('d1'), r.bridge.send('d1')])
  assert.ok(a.ok && b.ok)
  assert.deepEqual(r.net.sent, [{ to: '~nec', body: 'hello' }])
  assert.equal(r.get('d1').fields.status, 'pending')
  assert.equal(r.get('d1').fields.mid, 1)
  assert.equal((await r.bridge.send('d1')).ok, false)
  assert.equal(r.net.sent.length, 1)
})

test('the ship’s status reaches the message, and a retry isn’t a duplicate', async () => {
  const r = rig()
  r.net.fail = 'peer is blocked'
  const no = await r.bridge.send('d1')
  assert.match(no.say, /wasn’t sent/)
  assert.equal(r.get('d1').fields.status, 'failed')
  r.net.fail = null
  await r.bridge.send('d1')
  r.net.all[0].status = 'failed'
  await r.bridge.poll()
  assert.equal(r.get('d1').fields.status, 'failed')
  await r.bridge.send('d1')
  r.net.all[1].status = 'delivered'
  await r.bridge.poll()
  assert.equal(r.get('d1').fields.status, 'delivered')
  assert.deepEqual(r.get('t1').fields.items, ['d1'])
})

test('incoming messages become objects once, even across a restart', async () => {
  const r = rig()
  r.net.all.push({ id: 9, peer: '~nec', outgoing: false, body: 'yo', status: 'received', sent: '~2026.11.2..10.00.00', group: null })
  await r.bridge.poll()
  await r.bridge.poll()
  const again = createMessages({ call: r.call, api: r.api, log: () => {} })
  await again.poll()
  assert.deepEqual(r.get('t1').fields.items, ['d1', 'msg:~nec:9'])
  assert.equal(r.get('t1').fields.unread, 1)
})

test('a new peer gets a thread in Messages', async () => {
  const r = rig()
  r.net.all.push({ id: 1, peer: '~bud', outgoing: false, body: 'hey', status: 'received', sent: '~2026.11.2..10.00.00', group: null })
  await r.bridge.poll()
  assert.equal(r.get('thread:~bud').fields.peer, '~bud')
  assert.ok(r.get('messages').fields.items.includes('thread:~bud'))
})

test('a send interrupted before its id was saved is claimed, not duplicated', async () => {
  const r = rig()
  await r.call('POST', '/do', { do: 'batch', ops: [{ do: 'set', id: 'd1', base: 0, fields: { status: 'sending' } }] })
  r.net.all.push({ id: 5, peer: '~nec', outgoing: true, body: 'hello', status: 'delivered', sent: '~2026.11.2..10.00.00', group: null })
  await r.bridge.poll()
  assert.equal(r.get('d1').fields.mid, 5)
  assert.equal(r.get('d1').fields.status, 'delivered')
  assert.deepEqual(r.get('t1').fields.items, ['d1'])
})

test('send refuses the oversized and the peerless', async () => {
  const r = rig()
  await r.call('POST', '/do', { do: 'batch', ops: [{ do: 'set', id: 'd1', base: 0, fields: { text: 'x'.repeat(8193) } }] })
  assert.match((await r.bridge.send('d1')).say, /limit/)
  await r.call('POST', '/do', { do: 'batch', ops: [{ do: 'set', id: 'd1', base: 0, fields: { text: 'ok' } }, { do: 'set', id: 't1', base: 0, fields: { peer: '' } }] })
  assert.match((await r.bridge.send('d1')).say, /no ship/)
  assert.equal(r.net.sent.length, 0)
})

test('an agent can draft through a proposal, and nothing it does delivers', async () => {
  const r = rig()
  const model = async () => ({ text: JSON.stringify({ reply: 'Drafted.', ops: [
    { do: 'make', id: 'd2', type: 'message', fields: { text: 'from the agent', from: 'me', status: 'draft' } },
    { do: 'splice', id: 't1', field: 'items', del: [], add: [{ after: 'd1', val: 'd2' }] },
  ] }), cost: 0 })
  const agent = createAgent({ call: r.call, model })
  await agent.ask({ text: 'write to nec', selection: [] })
  const p = Object.values(r.ship.data.objects).find((o) => o.type === 'proposal')
  assert.equal(r.get('d2'), undefined)
  const [code] = r.ship.http('POST', '/store/~/v1/do', { do: 'batch', actor: 'agent:claude', ops: [{ do: 'set', id: 'd1', fields: { status: 'pending' } }] })
  assert.notEqual(code, 200)
  assert.equal(r.ship.http('POST', '/store/~/v1/do', { do: 'approve', id: p.id })[0], 200)
  await r.bridge.poll()
  assert.equal(r.get('d2').fields.status, 'draft')
  assert.equal(r.net.sent.length, 0)
})

test('the demo fake delivers nothing real and answers in script', async () => {
  const api = fakeMessages()
  const { id } = await api('POST', '/messages', { to: '~nec', body: 'hi' })
  assert.ok(id)
  assert.equal((await api('GET', '/conversations')).conversations[0].peer, '~nec')
})
