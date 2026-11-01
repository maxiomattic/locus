// Between the engine and the ship on this device. The engine edits its copy at
// once; this turns each difference into typed operations and folds in changes
// from elsewhere. Nothing here decides what a change means.
const API = '/store/~/v1'
const META = (k) => k.startsWith('$') || k === 'id' || k === 'type'

export const toLocal = (o) => ({
  ...o.fields, id: o.id, type: o.type, $rev: o.rev, $at: o.at,
  ...(o.trashed ? { $trashed: true } : {}), ...(o.clashes?.length ? { $clashes: o.clashes } : {}),
})

const same = (a, b) => a === b || (typeof a === 'object' && typeof b === 'object' && JSON.stringify(a) === JSON.stringify(b))

function splice(id, field, was, now) {
  const stays = was.filter((x) => now.includes(x))
  if (JSON.stringify(stays) !== JSON.stringify(now.filter((x) => was.includes(x)))) return null
  const add = []
  now.forEach((x, i) => { if (!was.includes(x)) add.push({ after: i ? now[i - 1] : null, val: x }) })
  return { do: 'splice', id, field, del: was.filter((x) => !now.includes(x)), add }
}

// The operations that turn one store into another.
export function diff(prev, next, revs = {}) {
  const ops = []
  for (const id in next) {
    const a = prev[id]
    const b = next[id]
    if (a === b) continue
    const fields = {}
    if (!a) {
      for (const k in b) if (!META(k) && b[k] !== undefined) fields[k] = b[k]
      ops.push({ do: 'make', id, type: b.type, fields })
      continue
    }
    for (const k in b) {
      if (META(k) || same(a[k], b[k])) continue
      const list = Array.isArray(a[k]) && Array.isArray(b[k]) && k === 'items' ? splice(id, k, a[k], b[k]) : null
      if (list) ops.push(list)
      else fields[k] = b[k] ?? null
    }
    for (const k in a) if (!META(k) && !(k in b)) fields[k] = null
    if (Object.keys(fields).length) ops.push({ do: 'set', id, base: revs[id] ?? a.$rev ?? 0, fields })
  }
  for (const id in prev) if (!(id in next)) ops.push({ do: 'trash', id })
  return ops
}

export function describe(op, store) {
  const name = (id) => store[id]?.title ?? store[id]?.text ?? store[id]?.name ?? id
  const val = (v) => (typeof v === 'string' ? `“${v.slice(0, 60)}”` : JSON.stringify(v))
  if (op.do === 'make') return `Make a ${op.type}: ${val(op.fields.title ?? op.fields.text ?? op.fields.name ?? op.id)}`
  if (op.do === 'set') return `${name(op.id)}: ${Object.entries(op.fields).map(([k, v]) => `${k} to ${val(v)}`).join(', ')}`
  if (op.do === 'splice') return `${name(op.id)}: ${[...op.add.map((a) => `add ${name(a.val)}`), ...op.del.map((d) => `remove ${name(d)}`)].join(', ')}`
  if (op.do === 'trash') return `Move ${name(op.id)} to the trash`
  return `${op.do} ${name(op.id)}`
}

const post = (url, body) => fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
  .then(async (r) => ({ ok: r.ok, data: await r.json().catch(() => ({})) }))

export function connect(initial, ship) {
  return (get, ui) => {
    let synced = initial
    const revs = Object.fromEntries(Object.values(initial).map((o) => [o.id, o.$rev]))
    const plain = (o) => o && JSON.stringify(Object.keys(o).filter((k) => !k.startsWith('$')).sort().map((k) => [k, o[k]]))
    let ver = 0
    let timer = null
    let busy = false
    let stopped = false
    let n = 0
    const binned = new Set()
    const net = { state: 'online', pending: 0, ship }
    const report = () => ui.status({ ...net })

    // Sends whatever differs from the last confirmed copy, so a burst of keys
    // becomes one change.
    async function flush() {
      timer = null
      if (busy) return void (timer = setTimeout(flush, 100))
      const store = get().store
      if (store === synced) return
      const ops = diff(synced, store, revs).flatMap((op) => {
        if (op.do === 'trash') binned.add(op.id)
        if (op.do === 'make' && binned.delete(op.id)) return [{ do: 'restore', id: op.id }, { do: 'set', id: op.id, base: 0, fields: op.fields }]
        return [op]
      })
      if (!ops.length) { synced = store; return }
      busy = true
      const cid = `c${Date.now().toString(36)}${++n}`
      const r = await post(`${API}/do`, { do: 'batch', cid, intent: get().undo.at(-1)?.label ?? 'edit', ops }).catch(() => null)
      busy = false
      if (r?.ok) {
        for (const op of ops) revs[op.id] = (revs[op.id] ?? 0) + 1
        synced = store
        net.state = 'online'
      } else if (r) {
        synced = store
        ui.say(`The ship refused that: ${r.data.error ?? 'unknown error'}`)
      } else {
        net.state = 'offline'
        timer = setTimeout(flush, 3000)
      }
      report()
    }

    async function poll() {
      while (!stopped) {
        try {
          const r = await fetch(`${API}/changes?since=${ver}`)
          const d = await r.json()
          if (ver) {
            const local = get().store
            // A change to something with unsent edits stays out, so those edits
            // go home against the revision they were made on and can clash.
            const fresh = d.objects.map(toLocal).filter((o) => o.$rev > (revs[o.id] ?? 0))
            const clean = fresh.filter((o) => !local[o.id] || !synced[o.id] || plain(local[o.id]) === plain(synced[o.id]))
            synced = { ...synced }
            clean.forEach((o) => { synced[o.id] = o; revs[o.id] = o.$rev })
            d.gone.forEach((id) => delete synced[id])
            ui.remote(clean, d.gone)
          }
          ver = d.ver
          net.pending = d.pending
          net.state = 'online'
        } catch { net.state = 'offline' }
        report()
        await new Promise((go) => setTimeout(go, 700))
      }
    }
    poll()

    // Follow what the agent is changing, one object at a time, in its tile.
    const follow = async (ids) => {
      for (const id of ids.slice(0, 12)) {
        ui.watch(id)
        await new Promise((go) => setTimeout(go, 500))
      }
    }

    return {
      stop() { stopped = true },
      changed() { if (!timer) timer = setTimeout(flush, 250) },
      async send(e) {
        if (e.do === 'ask') {
          const r = await post('/agent/ask', e).catch(() => null)
          ui.say(r?.data.say ?? 'The agent can’t be reached.', { working: null })
        } else if (e.do === 'tool') {
          const r = await post('/agent/tool', e).catch(() => null)
          ui.say(r?.data.say ?? 'That tool failed.')
          if (r?.data.touched) follow(r.data.touched)
        } else if (e.do === 'decide') {
          const o = get().store[e.id]
          const r = await post(`${API}/do`, { do: e.ok ? 'approve' : 'reject', id: e.id }).catch(() => null)
          if (!r?.ok) return ui.say('The ship refused that.')
          if (e.ok) follow([...new Set((o.ops ?? []).map((op) => op.id))])
        } else if (e.do === 'history') {
          const log = await fetch(`${API}/log?since=0`).then((r) => r.json()).catch(() => [])
          ui.history(log.reverse().slice(0, 200))
        } else if (e.do === 'revert' || e.do === 'restore' || e.do === 'resolve') {
          const body = e.do === 'revert' ? e
            : { do: 'batch', intent: e.do, ops: [e.do === 'restore' ? { do: 'restore', id: e.id } : { do: 'resolve', id: e.id, field: e.field, keep: e.keep }] }
          const r = await post(`${API}/do`, body).catch(() => null)
          if (!r?.ok) ui.say(r?.data.error ?? 'That failed.')
        }
      },
    }
  }
}
