// The public demo's stand-in for a ship: %store's rules (desk/lib/store.hoon and
// desk/app/store.hoon) in plain JavaScript, so the real interface and agent run
// in a visitor's browser with no server behind them. Home-only: no replication,
// and no reactions, which nothing in the demo sets up.
const clone = (v) => structuredClone(v)
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)
const list = (j) => (Array.isArray(j) ? j : [])
const asNum = (j) => (typeof j === 'number' ? Math.trunc(j) : 0)
const FUEL = 200000

export const idOf = (op) => op.id

function typeError(objs, type, fields) {
  const decl = objs[`type:${type}`]?.fields.fields
  if (!decl || typeof decl !== 'object') return null
  for (const [k, v] of Object.entries(fields)) {
    if (v === null || v === undefined) continue
    const kind = decl[k]
    const fits = {
      text: typeof v === 'string', ref: typeof v === 'string', blob: typeof v === 'string',
      num: typeof v === 'number', bool: typeof v === 'boolean', list: Array.isArray(v), refs: Array.isArray(v),
    }[kind]
    if (fits === false) return `bad value for ${k} in ${type}`
  }
  return null
}

function insert(l, after, val) {
  if (after === null || after === undefined) return [val, ...l]
  const at = l.findIndex((x) => same(x, after))
  if (at < 0) return [...l, val]
  return [...l.slice(0, at + 1), val, ...l.slice(at + 1)]
}

function write(o, seq, actor, base, fresh) {
  const now = o.rev + 1
  for (const [k, v] of Object.entries(fresh)) {
    const have = o.fields[k] ?? null
    if (same(have, v ?? null)) continue
    if (base !== 0 && (o.clock[k] ?? 0) > base) {
      o.clashes = [{ field: k, theirs: v, actor, seq }, ...o.clashes]
      continue
    }
    o.clock[k] = now
    if (v === null || v === undefined) delete o.fields[k]
    else o.fields[k] = v
  }
}

// One operation, on a store this batch owns. Returns its undo, or throws.
function applyOne(objs, seq, at, actor, op) {
  if (op.do === 'make') {
    if (!op.id) throw new Error('empty id')
    if (objs[op.id]) throw new Error(`exists: ${op.id}`)
    const bad = typeError(objs, op.type, op.fields)
    if (bad) throw new Error(bad)
    const fields = Object.fromEntries(Object.entries(op.fields).filter(([, v]) => v !== null))
    objs[op.id] = {
      id: op.id, type: op.type, rev: 1, fields, trashed: false, clashes: [], by: actor, at,
      clock: Object.fromEntries(Object.keys(op.fields).map((k) => [k, 1])),
    }
    return [{ do: 'purge', id: op.id }]
  }
  if (!objs[op.id]) throw new Error(`no such object: ${op.id}`)
  const o = objs[op.id] = clone(objs[op.id])
  const stamp = () => { o.rev++; o.by = actor; o.at = at }
  switch (op.do) {
    case 'set': {
      const bad = typeError(objs, o.type, op.fields)
      if (bad) throw new Error(bad)
      const prior = Object.fromEntries(Object.keys(op.fields).map((k) => [k, o.fields[k] ?? null]))
      write(o, seq, actor, op.base ?? 0, op.fields)
      stamp()
      return [{ do: 'set', id: op.id, base: 0, fields: prior }]
    }
    case 'splice': {
      const was = list(o.fields[op.field])
      let next = was.filter((x) => !list(op.del).some((d) => same(d, x)))
      for (const a of list(op.add)) next = insert(next, a.after ?? null, a.val)
      o.fields[op.field] = next
      o.clock[op.field] = o.rev + 1
      stamp()
      return [{ do: 'set', id: op.id, base: 0, fields: { [op.field]: was } }]
    }
    case 'trash': o.trashed = true; stamp(); return [{ do: 'restore', id: op.id }]
    case 'restore': o.trashed = false; stamp(); return [{ do: 'trash', id: op.id }]
    case 'resolve': {
      const mine = o.clashes.filter((c) => c.field === op.field)
      if (!mine.length) throw new Error('no such clash')
      o.clashes = o.clashes.filter((c) => c.field !== op.field)
      if (op.keep) { o.fields[op.field] = mine[0].theirs; o.clock[op.field] = o.rev + 1 }
      stamp()
      return []
    }
    case 'purge': delete objs[op.id]; return []
    default: throw new Error('bad operations')
  }
}

const typeOf = (objs, op) => (op.do === 'make' ? op.type : objs[op.id]?.type)
const sensitive = (objs, op) => op.do === 'purge' || ['grant', 'type', 'member'].includes(typeOf(objs, op))
const listed = (j, t) => list(j).includes(t) || list(j).includes('*')
const granted = (objs, actor, op) => Object.values(objs).some((o) => o.type === 'grant' && !o.trashed
  && o.fields.actor === actor && listed(o.fields.ops, op.do) && listed(o.fields.types, typeOf(objs, op)))

function allowed(objs, actor, ops) {
  if (actor === 'human') return
  if (ops.some((op) => sensitive(objs, op))) throw new Error('an agent cannot change grants or types')
  if (!ops.every((op) => (op.do === 'make' && op.type === 'proposal') || granted(objs, actor, op))) throw new Error('needs approval')
}

// ---- views ----
function matches(o, w) {
  const have = w.field === 'id' ? o.id : (o.fields[w.field] ?? null)
  if ('is' in w) return same(have, w.is)
  if ('has' in w) {
    if (Array.isArray(have)) return have.some((x) => same(x, w.has))
    return typeof have === 'string' && typeof w.has === 'string' && have.toLowerCase().includes(w.has.toLowerCase())
  }
  if ('gt' in w) return asNum(have) > asNum(w.gt)
  if ('lt' in w) return asNum(have) < asNum(w.lt)
  return false
}

export function query(objs, q) {
  if (!q || typeof q !== 'object') return []
  let all = Object.values(objs).filter((o) => !o.trashed && (!q.type || o.type === q.type) && list(q.where).every((w) => matches(o, w)))
  const by = (o) => (q.sort ? o.fields[q.sort] ?? '' : o.id)
  all.sort((a, b) => { const x = by(a); const y = by(b); return x < y ? -1 : x > y ? 1 : 0 })
  if (q.desc === true) all.reverse()
  if (typeof q.limit === 'number') all = all.slice(0, q.limit)
  return all
}

// ---- the tool language ----
const truthy = (j) => !(j === null || j === undefined || j === false || j === '' || (Array.isArray(j) && !j.length))

function expr(objs, e, env) {
  if (!e || typeof e !== 'object' || Array.isArray(e)) return e ?? null
  const ev = (j) => expr(objs, j, env)
  if ('var' in e) return env[e.var] ?? null
  if ('lit' in e) return e.lit
  if ('obj' in e) return Object.fromEntries(Object.entries(e.obj ?? {}).map(([k, v]) => [k, ev(v)]))
  if ('arr' in e) return list(e.arr).map(ev)
  if ('get' in e) {
    const [a, b] = list(e.get)
    const id = ev(a)
    const f = ev(b)
    const o = objs[id]
    if (!o) return null
    return f === 'id' ? id : f === 'type' ? o.type : o.fields[f] ?? null
  }
  if ('query' in e) return query(objs, ev(e.query)).map((o) => o.id)
  if ('concat' in e) return list(e.concat).map((j) => { const x = ev(j); return typeof x === 'string' || typeof x === 'number' ? x : '' }).join('')
  if ('lower' in e) { const x = ev(e.lower); return typeof x === 'string' ? x.toLowerCase() : null }
  if ('eq' in e) return same(ev(e.eq[0]), ev(e.eq[1]))
  if ('not' in e) return !truthy(ev(e.not))
  if ('len' in e) return list(ev(e.len)).length
  if ('if' in e) { const [c, a, b] = list(e.if); return truthy(ev(c)) ? ev(a) : ev(b) }
  if ('plus' in e) return asNum(ev(e.plus[0])) + asNum(ev(e.plus[1]))
  if ('mod' in e) { const d = asNum(ev(e.mod[1])); return d ? asNum(ev(e.mod[0])) % d : null }
  return e
}

function steps(objs, ls, r, salt) {
  for (const s of ls) {
    if (r.fuel-- <= 0) throw new Error('out of fuel')
    if (!s || typeof s !== 'object') throw new Error('a step must be an object')
    const sub = (a, k) => expr(objs, a?.[k], r.env)
    if ('let' in s) r.env[s.let] = expr(objs, s.be, r.env)
    else if ('for' in s) {
      for (const x of list(expr(objs, s.for, r.env))) {
        r.env[s.as ?? 'it'] = x
        steps(objs, list(s.do), r, salt)
      }
    } else if ('when' in s) { if (truthy(expr(objs, s.when, r.env))) steps(objs, list(s.do), r, salt) }
    else if ('make' in s) {
      const type = sub(s.make, 'type')
      if (typeof type !== 'string') throw new Error('make needs a type')
      const id = sub(s.make, 'id')
      r.ops.push({ do: 'make', id: typeof id === 'string' ? id : `${salt}-${r.n}`, type, fields: sub(s.make, 'fields') ?? {} })
      r.n++
    } else if ('set' in s) r.ops.push({ do: 'set', id: sub(s.set, 'id'), base: 0, fields: sub(s.set, 'fields') ?? {} })
    else if ('add' in s) r.ops.push({ do: 'splice', id: sub(s.add, 'id'), field: sub(s.add, 'field'), del: [], add: [{ after: null, val: sub(s.add, 'value') }] })
    else if ('del' in s) r.ops.push({ do: 'splice', id: sub(s.del, 'id'), field: sub(s.del, 'field'), del: [sub(s.del, 'value')], add: [] })
    else if ('trash' in s) r.ops.push({ do: 'trash', id: expr(objs, s.trash, r.env) })
    else throw new Error('unknown step')
  }
}

export function runTool(objs, prog, args, salt) {
  const r = { env: { ...args }, ops: [], fuel: FUEL, n: 0 }
  steps(objs, list(prog), r, salt)
  return r.ops
}

function effects(prog, out = new Set()) {
  for (const s of list(prog)) {
    if (!s || typeof s !== 'object') continue
    effects(s.do, out)
    for (const k of ['set', 'add', 'del', 'trash']) if (k in s) out.add(k === 'add' || k === 'del' ? 'splice' : k)
    if ('make' in s) out.add(typeof s.make?.type === 'string' ? `make:${s.make.type}` : 'make:*')
  }
  return out
}

const overReach = (can, prog) => [...effects(prog)].filter((e) => !list(can).includes(e))

function checkTool(objs, t) {
  const over = overReach(t.fields.can, t.fields.program)
  if (over.length) return `program exceeds what it declares: ${over[0]}`
  for (const ex of list(t.fields.examples)) {
    const got = runTool(objs, t.fields.program, ex.args ?? {}, 'check').map((o) => o.do)
    if (!same(list(ex.expect), got)) return 'an example gave different operations'
  }
  return null
}

// ---- the ship ----
export function createShip(saved, { save = () => {}, now = () => Date.now() } = {}) {
  const s = saved ?? { objects: {}, log: [], seq: 0, ver: 0, touched: {} }
  const secs = () => Math.floor(now() / 1000)
  const rand = () => Math.random().toString(36).slice(2, 12)

  function commit({ cid = '', actor = 'human', intent = '', ops }) {
    allowed(s.objects, actor, ops)
    const seq = s.seq + 1
    const at = secs()
    const objs = { ...s.objects }
    let undo = []
    for (const op of ops) undo = [...applyOne(objs, seq, at, actor, op), ...undo]
    s.objects = objs
    s.seq = seq
    s.log.push({ seq, at, who: '~sampel-palnet', actor, intent, cid, ops, undo })
    if (s.log.length > 500) s.log.shift()
    for (const id of ops.filter((o) => o.do === 'purge').map((o) => o.id)) {
      for (const e of s.log) {
        e.ops = e.ops.filter((o) => o.id !== id)
        e.undo = e.undo.filter((o) => o.id !== id)
      }
    }
    s.ver++
    for (const op of ops) s.touched[op.id] = s.ver
    save(s)
    return seq
  }

  function settle(id, ok) {
    const o = s.objects[id]
    if (!o) throw new Error('no such proposal')
    if (o.type !== 'proposal') throw new Error('not a proposal')
    if (o.fields.status !== 'open') throw new Error('already decided')
    commit({
      intent: `${ok ? 'approved' : 'rejected'}: ${o.fields.intent}`,
      ops: [...(ok ? list(o.fields.ops) : []), { do: 'set', id, base: 0, fields: { status: ok ? 'applied' : 'rejected' } }],
    })
  }

  function act(b) {
    const actor = b.actor || 'human'
    switch (b.do) {
      case 'batch': return commit({ cid: b.cid, actor, intent: b.intent, ops: b.ops })
      case 'propose':
        return commit({ actor, intent: b.intent, ops: [{ do: 'make', id: `proposal:${rand()}`, type: 'proposal', fields: { actor, intent: b.intent ?? '', ops: b.ops ?? [], status: 'open' } }] })
      case 'approve': settle(b.id, true); return null
      case 'reject': settle(b.id, false); return null
      case 'revert': {
        const e = s.log.find((x) => x.seq === b.seq)
        if (!e) throw new Error('no such entry')
        return commit({ intent: `undo: ${e.intent}`, ops: e.undo })
      }
      case 'run-tool': {
        const o = s.objects[b.id]
        if (!o) throw new Error('no such tool')
        const bad = checkTool(s.objects, o)
        if (bad) throw new Error(bad)
        return commit({ cid: b.cid, actor, intent: `${o.fields.name} (tool:${o.id})`, ops: runTool(s.objects, o.fields.program, b.args ?? {}, rand()) })
      }
      case 'try-tool': {
        const over = overReach(b.can, b.program)
        if (over.length) throw new Error(`program exceeds what it declares: ${over[0]}`)
        return { ops: runTool(s.objects, b.program, b.args ?? {}, 'try'), can: [...effects(b.program)] }
      }
      case 'check-tool': {
        const o = s.objects[b.id]
        if (!o) throw new Error('no such tool')
        const bad = checkTool(s.objects, o)
        return { ok: !bad, why: bad ?? '' }
      }
      case 'query': return query(s.objects, b.q)
      default: throw new Error('unknown action')
    }
  }

  const wire = ({ clock, ...o }) => o
  const state = () => ({ ver: s.ver, seq: s.seq, home: '~sampel-palnet', ship: '~sampel-palnet', pending: 0, objects: Object.values(s.objects).map(wire) })
  const changes = (since) => {
    const ids = Object.keys(s.touched).filter((i) => s.touched[i] > since)
    return {
      ver: s.ver, seq: s.seq, pending: 0, rejected: [],
      objects: ids.filter((i) => s.objects[i]).map((i) => wire(s.objects[i])),
      gone: ids.filter((i) => !s.objects[i]),
    }
  }

  // The same answers %store gives over HTTP.
  function http(method, url, body) {
    const u = new URL(url, 'http://ship')
    const path = u.pathname.replace('/store/~/v1', '')
    const since = Number(u.searchParams.get('since') ?? 0)
    if (path === '/state') return [200, state()]
    if (path === '/changes') return [200, changes(since)]
    if (path === '/log') return [200, s.log.filter((e) => e.seq > since).slice(0, 500)]
    if (path === '/do' && method === 'POST') {
      try { return [200, { ok: true, result: act(body) ?? null }] } catch (e) { return [409, { ok: false, error: e.message }] }
    }
    return [404, 'not found']
  }

  // Filling an empty ship.
  function load(ops) { commit({ intent: 'seed the store', ops }) }

  return { http, load, data: s }
}
