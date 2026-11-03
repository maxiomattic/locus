// Between %messages, which talks to other ships, and %store, which the interface
// reads. Delivery happens here and nowhere else: only send() reaches the network,
// and only the person's own key calls it. Agents can draft, never deliver.
const ACTOR = 'agent:host'
const MAX = 8192
const PEER = /^~(?:[a-z]{3}|[a-z]{6})(?:-[a-z]{6})*(?:--[a-z]{6}(?:-[a-z]{6})*)?$/
const STATUS = { pending: 'pending', delivered: 'delivered', failed: 'failed', received: 'received' }

// What a store needs before the bridge can write to it. The host may list a new
// thread in Messages, so it can splice collections.
export const messageSetup = (kind) => [
  kind({ name: 'thread', fields: { title: 'text', peer: 'text', items: 'refs' } }),
  kind({ name: 'message', fields: { text: 'text', from: 'text', status: 'text' } }),
  { do: 'make', id: 'grant:host-messages', type: 'grant', fields: { actor: ACTOR, ops: ['make', 'set', 'splice'], types: ['thread', 'message'] } },
  { do: 'make', id: 'grant:host-message-list', type: 'grant', fields: { actor: ACTOR, ops: ['splice'], types: ['collection'] } },
]

// ~2026.11.2..10.00.00..abcd to seconds.
export function fromDa(da) {
  const m = /^~(\d+)\.(\d+)\.(\d+)(?:\.\.(\d+)\.(\d+)\.(\d+))?/.exec(da ?? '')
  if (!m) return Math.floor(Date.now() / 1000)
  return Date.UTC(+m[1], +m[2] - 1, +m[3], +(m[4] ?? 0), +(m[5] ?? 0), +(m[6] ?? 0)) / 1000
}

export const shipApi = (ship) => async (method, url, body) => {
  const r = await ship(`/messages/~/v1${url}`, method === 'POST'
    ? { method, headers: { 'content-type': 'application/json' }, body: JSON.stringify(body ?? {}) } : {})
  const d = await r.json().catch(() => ({}))
  if (!r.ok || d.ok === false) throw new Error(d.message ?? d.error ?? `the ship answered ${r.status}`)
  return d
}

export function createMessages({ call, api, log = console.error }) {
  let cache = null // what the store holds, rebuilt after anything changes it
  let chain = Promise.resolve()
  let timer = null
  const seen = new Map() // peer -> how its conversation last looked
  const inFlight = new Set()
  const locked = (fn) => {
    const run = chain.then(fn, fn)
    chain = run.catch(() => {})
    return run
  }
  const act = (intent, ops) => call('POST', '/do', { do: 'batch', actor: ACTOR, intent, ops })

  async function load() {
    const state = await call('GET', '/state')
    const objects = Object.fromEntries(state.objects.map((o) => [o.id, o]))
    const threads = new Map()
    const mids = new Map()
    const owner = new Map()
    for (const o of state.objects) {
      if (o.type !== 'thread') continue
      if (o.fields.peer && !threads.has(o.fields.peer)) threads.set(o.fields.peer, o.id)
      for (const id of o.fields.items ?? []) owner.set(id, o)
    }
    for (const o of state.objects) {
      if (o.type !== 'message') continue
      const peer = owner.get(o.id)?.fields.peer
      for (const mid of [o.fields.mid, ...(o.fields.priorMids ?? [])]) if (mid != null) mids.set(`${peer}:${mid}`, o.id)
    }
    cache = { objects, threads, mids, owner }
    return cache
  }

  // A message the ship has but no object knows: sent just before a crash. The
  // message left stuck in sending claims it, so it doesn't appear twice.
  function claim(c, peer, m) {
    const t = c.objects[c.threads.get(peer)]
    return (t?.fields.items ?? []).map((id) => c.objects[id])
      .find((o) => o?.fields.status === 'sending' && o.fields.text === m.body)
  }

  async function pollOnce() {
    const c = cache ?? await load()
    const { conversations } = await api('GET', '/conversations')
    const live = new Set(Object.values(c.objects)
      .filter((o) => o.type === 'message' && ['pending', 'sending'].includes(o.fields.status))
      .map((o) => c.owner.get(o.id)?.fields.peer))
    const ops = []
    const looks = []
    for (const v of conversations) {
      const look = JSON.stringify([v.count, v.unread, v.last?.id, v.last?.status])
      if (seen.get(v.peer) === look && !live.has(v.peer)) continue
      const { messages } = await api('GET', `/conversations/${v.peer}`)
      const tid = c.threads.get(v.peer) ?? `thread:${v.peer}`
      const fresh = !c.threads.has(v.peer)
      const made = []
      let tail = c.objects[tid]?.fields.items?.at(-1) ?? null
      for (const m of messages) {
        if (m.group) continue // Group conversations have no surface yet.
        const known = c.mids.get(`${v.peer}:${m.id}`)
        const status = STATUS[m.status] ?? 'pending'
        if (known) {
          if (c.objects[known]?.fields.status !== status) ops.push({ do: 'set', id: known, base: 0, fields: { status } })
          continue
        }
        const stuck = m.outgoing ? claim(c, v.peer, m) : null
        if (stuck) {
          ops.push({ do: 'set', id: stuck.id, base: 0, fields: { status, mid: m.id, at: fromDa(m.sent) } })
          continue
        }
        const id = `msg:${v.peer}:${m.id}`
        ops.push({ do: 'make', id, type: 'message', fields: { text: m.body, from: m.outgoing ? 'me' : v.peer, status, mid: m.id, at: fromDa(m.sent) } })
        if (!fresh) ops.push({ do: 'splice', id: tid, field: 'items', del: [], add: [{ after: tail, val: id }] })
        made.push(id)
        tail = id
      }
      // The ship checks a batch against the store before it runs, so a new thread
      // has to be made whole.
      if (fresh) {
        ops.push(
          { do: 'make', id: tid, type: 'thread', fields: { title: v.peer, peer: v.peer, items: made, unread: v.unread } },
          { do: 'splice', id: 'messages', field: 'items', del: [], add: [{ after: null, val: tid }] },
        )
      } else if ((c.objects[tid]?.fields.unread ?? 0) !== v.unread) {
        ops.push({ do: 'set', id: tid, base: 0, fields: { unread: v.unread } })
      }
      looks.push([v.peer, look])
    }
    if (ops.length) {
      await act('messages from the ship', ops)
      cache = null
    }
    for (const [p, look] of looks) seen.set(p, look)
  }

  const poll = () => locked(pollOnce)

  const deliver = (id) => locked(async () => {
    const c = await load()
    const m = c.objects[id]
    const t = c.owner.get(id)
    if (m?.type !== 'message' || !t) return { ok: false, say: 'That message isn’t in a thread.' }
    if (!['draft', 'failed'].includes(m.fields.status)) return { ok: false, say: 'That message has already gone.' }
    const text = m.fields.text ?? ''
    if (!text.trim()) return { ok: false, say: 'That message is empty.' }
    if (new TextEncoder().encode(text).length > MAX) return { ok: false, say: `That message is over the ${MAX} byte limit.` }
    if (!PEER.test(t.fields.peer ?? '')) return { ok: false, say: 'This thread has no ship to send to.' }
    await act('send a message', [{ do: 'set', id, base: 0, fields: { status: 'sending' } }])
    try {
      const r = await api('POST', '/messages', { to: t.fields.peer, body: text })
      const prior = m.fields.mid != null ? [...(m.fields.priorMids ?? []), m.fields.mid] : m.fields.priorMids
      await act('message sent', [{ do: 'set', id, base: 0, fields: { status: 'pending', mid: r.id, at: Math.floor(Date.now() / 1000), ...(prior ? { priorMids: prior } : {}) } }])
      return { ok: true, say: `Sent to ${t.fields.peer}.` }
    } catch (e) {
      await act('message failed', [{ do: 'set', id, base: 0, fields: { status: 'failed' } }])
      return { ok: true, say: `That wasn’t sent: ${e.message}. Press A to try again.` }
    } finally {
      cache = null
    }
  })

  return {
    poll,
    async send(id) {
      if (inFlight.has(id)) return { ok: true, say: 'Already sending.' }
      inFlight.add(id)
      try { return await deliver(id) } finally { inFlight.delete(id) }
    },
    async read(id) {
      const c = await locked(load)
      const peer = c.objects[id]?.fields.peer
      if (peer) await api('POST', `/conversations/${peer}/read`).catch((e) => log(e))
      return { ok: true }
    },
    start(every = 4000) {
      const go = async () => {
        await poll().catch((e) => log(`messages: ${e.message}`))
        timer = setTimeout(go, every)
      }
      go()
    },
    stop() { clearTimeout(timer) },
  }
}
