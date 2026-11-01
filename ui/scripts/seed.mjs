// Fills an empty ship's store: the Home layout, sample notes, the stock photos
// and the movie library. Safe to run once; a store with a home is left alone.
import fs from 'node:fs'
import { seedStore } from '../src/data.js'
import { withMedia } from '../src/media.js'

const url = process.env.LOCUS_URL ?? 'http://127.0.0.1:5180'
const state = await fetch(`${url}/store/~/v1/state`).then((r) => r.json())
if (state.objects.some((o) => o.id === 'home')) { console.log('already seeded'); process.exit(0) }
const manifest = await fetch(`${url}/media/manifest.json`).then((r) => (r.ok ? r.json() : null), () => null)
const store = manifest ? withMedia(seedStore(), manifest) : seedStore()
// LOCUS_REAL=1: a store for real data, without the sample notes and people.
if (process.env.LOCUS_REAL) {
  for (const key of ['n', 'e']) {
    const tile = Object.values(store).find((o) => o.launch === key)
    for (const id of tile.items) {
      for (const b of store[id].items ?? []) delete store[b]
      delete store[id]
    }
    tile.items = []
  }
}
const ops = Object.values(store).map(({ id, type, ...fields }) => ({ do: 'make', id, type, fields }))
const KIND = (fields) => ({ do: 'make', id: `type:${fields.name}`, type: 'type', fields })
ops.push(
  KIND({ name: 'block', fields: { text: 'text', style: 'text', done: 'bool', ref: 'ref' } }),
  KIND({ name: 'note', fields: { title: 'text', items: 'refs' } }),
  KIND({ name: 'photo', fields: { title: 'text', rot: 'num', src: 'blob' } }),
  KIND({ name: 'collection', fields: { title: 'text', items: 'refs' } }),
  { do: 'make', id: 'grant:host-runs', type: 'grant', fields: { actor: 'agent:host', ops: ['make', 'set'], types: ['run'] } },
  { do: 'make', id: 'grant:host-tools', type: 'grant', fields: { actor: 'agent:host', ops: ['set'], types: ['tool'] } },
)
const SIZE = 250
for (let i = 0; i < ops.length; i += SIZE) {
  const r = await fetch(`${url}/store/~/v1/do`, {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ do: 'batch', intent: 'seed the store', ops: ops.slice(i, i + SIZE) }),
  })
  if (!r.ok) { console.error(await r.text()); process.exit(1) }
  console.log(`${Math.min(i + SIZE, ops.length)} of ${ops.length}`)
}
