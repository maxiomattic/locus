import test from 'node:test'
import assert from 'node:assert/strict'
import { createAgent } from '../server/agent.mjs'
import { createShip } from '../demo/ship.js'
import { demoModel } from '../demo/model.js'

const fresh = () => {
  const ship = createShip()
  ship.load([
    { do: 'make', id: 'a', type: 'photo', fields: { title: 'A', rot: 0 } },
    { do: 'make', id: 'b', type: 'photo', fields: { title: 'B', rot: 90 } },
    { do: 'make', id: 'n', type: 'note', fields: { title: 'N', items: [] } },
    { do: 'make', id: 'grant:runs', type: 'grant', fields: { actor: 'agent:host', ops: ['make', 'set'], types: ['run'] } },
    { do: 'make', id: 'grant:tools', type: 'grant', fields: { actor: 'agent:host', ops: ['set'], types: ['tool'] } },
  ])
  const call = async (method, url, body) => {
    const [code, data] = ship.http(method, `/store/~/v1${url}`, body)
    if (code !== 200) throw new Error(data.error)
    return data
  }
  return { ship, call, get: (id) => ship.data.objects[id], do: (b) => ship.http('POST', '/store/~/v1/do', b) }
}

test('a stale write keeps both: the newer value and a clash', () => {
  const s = fresh()
  s.do({ do: 'batch', ops: [{ do: 'set', id: 'n', base: 1, fields: { title: 'first' } }] })
  const [code] = s.do({ do: 'batch', ops: [{ do: 'set', id: 'n', base: 1, fields: { title: 'second' } }] })
  assert.equal(code, 200)
  assert.equal(s.get('n').fields.title, 'first')
  assert.equal(s.get('n').clashes[0].theirs, 'second')
  s.do({ do: 'batch', ops: [{ do: 'resolve', id: 'n', field: 'title', keep: true }] })
  assert.equal(s.get('n').fields.title, 'second')
  assert.equal(s.get('n').clashes.length, 0)
})

test('a batch is atomic', () => {
  const s = fresh()
  const [code] = s.do({ do: 'batch', ops: [{ do: 'set', id: 'n', fields: { title: 'x' } }, { do: 'set', id: 'nope', fields: {} }] })
  assert.equal(code, 409)
  assert.equal(s.get('n').fields.title, 'N')
})

test('revert undoes a logged change, in order', () => {
  const s = fresh()
  s.do({ do: 'batch', ops: [{ do: 'make', id: 'c', type: 'block', fields: { text: 'hi' } }, { do: 'splice', id: 'n', field: 'items', del: [], add: [{ after: null, val: 'c' }] }] })
  assert.deepEqual(s.get('n').fields.items, ['c'])
  s.do({ do: 'revert', seq: s.ship.data.seq })
  assert.deepEqual(s.get('n').fields.items, [])
  assert.equal(s.get('c'), undefined)
})

test('trash and restore', () => {
  const s = fresh()
  s.do({ do: 'batch', ops: [{ do: 'trash', id: 'a' }] })
  assert.equal(s.get('a').trashed, true)
  s.do({ do: 'batch', ops: [{ do: 'restore', id: 'a' }] })
  assert.equal(s.get('a').trashed, false)
})

test('an agent cannot write without approval, or touch grants at all', () => {
  const s = fresh()
  assert.equal(s.do({ do: 'batch', actor: 'agent:claude', ops: [{ do: 'set', id: 'n', fields: { title: 'x' } }] })[1].error, 'needs approval')
  assert.match(s.do({ do: 'batch', actor: 'agent:claude', ops: [{ do: 'trash', id: 'grant:runs' }] })[1].error, /cannot change/)
})

test('the changes feed names what moved and what went', () => {
  const s = fresh()
  const ver = s.ship.data.ver
  s.do({ do: 'batch', ops: [{ do: 'set', id: 'n', fields: { title: 'x' } }] })
  const [, d] = s.ship.http('GET', `/store/~/v1/changes?since=${ver}`)
  assert.deepEqual(d.objects.map((o) => o.id), ['n'])
  assert.deepEqual(d.gone, [])
})

test('the real agent proposes, then learns a tool on the second run, then needs no model', async () => {
  const s = fresh()
  let calls = 0
  const model = (p) => { calls++; return demoModel(p) }
  const agent = createAgent({ call: s.call, model })
  const first = await agent.ask({ text: 'rotate this', selection: ['a'] })
  assert.match(first.say, /Inbox/)
  assert.equal(s.get('a').fields.rot, 0)
  const open = () => Object.values(s.ship.data.objects).filter((o) => o.type === 'proposal' && o.fields.status === 'open')
  assert.equal(open().length, 1)
  assert.equal(s.do({ do: 'approve', id: open()[0].id })[0], 200)
  assert.equal(s.get('a').fields.rot, 90)

  const second = await agent.ask({ text: 'rotate this photo', selection: ['b'] })
  assert.equal(second.learned, true)
  for (const p of open()) assert.equal(s.do({ do: 'approve', id: p.id })[0], 200)
  assert.equal(s.get('b').fields.rot, 180)

  calls = 0
  const third = await agent.ask({ text: 'rotate this', selection: ['a'] })
  assert.equal(third.via, 'tool')
  assert.equal(calls, 0)
  assert.equal(s.get('a').fields.rot, 180)
})

test('the scripted agent names its limits instead of pretending', async () => {
  const r = JSON.parse((await demoModel('STORE: {}\nSELECTED: []\nREQUEST: write me a poem')).text)
  assert.match(r.reply, /not a live model/)
  assert.deepEqual(r.ops, [])
})
