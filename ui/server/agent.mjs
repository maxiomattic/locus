// The agent, beside the ship. It has no tools of its own and no way to change
// anything: it reads what it is shown and answers with operations, which go to
// the ship as a proposal. What it does twice becomes a tool that runs without it.
import { spawn } from 'node:child_process'

const STOP = new Set('a an and the this that these those it them my to of on in for please all with me can you i'.split(' '))
export const norm = (t) => t.toLowerCase().replace(/[^a-z0-9 ]/g, ' ').split(/\s+/).filter((w) => w && !STOP.has(w))
const jaccard = (a, b) => {
  const A = new Set(a)
  const B = new Set(b)
  const both = [...A].filter((x) => B.has(x)).length
  return both / (A.size + B.size - both || 1)
}

const LANGUAGE = `Operations (JSON):
{"do":"make","id":"<new id>","type":"note|block|collection|...","fields":{...}}
{"do":"set","id":"<id>","fields":{...}}  (null removes a field)
{"do":"splice","id":"<id>","field":"items","del":[...],"add":[{"after":<id or null>,"val":<id>}]}
{"do":"trash","id":"<id>"}
To add at the end of a list, "after" is its last id. A note has items, a list of block ids. A block has text and style (p, h, todo) and done for a to-do.
A collection has title, layout and items. A photo has title and rot (degrees).
An event has title, start and end (Unix seconds), repeat (null, daily or weekly) and items (blocks). To add one, make it with the owner's time zone in mind, then splice its id into the items of the object with id calendar.
A thread has title, peer and items, a list of message ids. A message has text, from and status. You can only draft: make a message with from "me" and status "draft" and add it to the end of the thread's items. Only the person can send it, and nothing you do can deliver it. Never set a message's status.`

const TOOL_LANGUAGE = `A tool program is a JSON list of steps:
{"let":"name","be":EXPR} {"for":EXPR,"as":"x","do":[steps]} {"when":EXPR,"do":[steps]}
{"make":{"type":"t","fields":{"obj":{...}}}} {"set":{"id":EXPR,"fields":{"obj":{"f":EXPR}}}}
{"add":{"id":EXPR,"field":"items","value":EXPR}} {"del":{...same}} {"trash":EXPR}
EXPR is a literal, or {"var":"x"}, {"get":[ID,"field"]}, {"query":{"type":"t"}}, {"concat":[...]},
{"plus":[a,b]}, {"mod":[a,b]}, {"lower":x}, {"eq":[a,b]}, {"not":x}, {"if":[c,a,b]}, {"len":x}, {"lit":x}.
Input: "selection", the list of selected ids. Nothing else can be done.`

export function claude(model = 'claude-haiku-5-5') {
  return (prompt) => new Promise((done, fail) => {
    const p = spawn('claude', ['-p', '--model', model, '--output-format', 'json', '--tools', '', '--no-session-persistence'], { cwd: '/tmp' })
    let out = ''
    p.stdout.on('data', (d) => { out += d })
    p.stderr.on('data', () => {})
    p.on('error', fail)
    p.on('close', () => {
      try {
        const r = JSON.parse(out)
        done({ text: r.result, cost: r.total_cost_usd ?? 0 })
      } catch { fail(new Error('the model gave no answer')) }
    })
    p.stdin.end(prompt)
  })
}

// Used by tests, so they run with no model and count how often one was needed.
export function stubModel() {
  const fn = async (prompt) => {
    fn.calls++
    const text = /^REQUEST: (.*)$/m.exec(prompt)?.[1] ?? ''
    const sel = JSON.parse(/^SELECTED: (.*)$/m.exec(prompt)?.[1] ?? '[]')
    if (/how many/.test(text)) return { text: JSON.stringify({ reply: 'Several.', ops: [] }), cost: 0.001 }
    if (/rotate/.test(text)) return { text: JSON.stringify({ reply: 'Turned them.', ops: sel.map((o) => ({ do: 'set', id: o.id, fields: { rot: ((o.rot ?? 0) + 90) % 360 } })) }), cost: 0.002 }
    const tag = /tag\b.*\b(\w+)$/.exec(text)
    if (tag) return { text: JSON.stringify({ reply: 'Tagged.', ops: sel.map((o) => ({ do: 'set', id: o.id, fields: { tag: tag[1] } })) }), cost: 0.002 }
    return { text: JSON.stringify({ reply: 'I can’t do that.', ops: [] }), cost: 0.001 }
  }
  fn.calls = 0
  return fn
}

const json = (t) => JSON.parse(t.slice(t.indexOf('{'), t.lastIndexOf('}') + 1))
const flat = (o) => ({ ...o.fields, id: o.id, type: o.type })
const brief = (o) => {
  const { ops, program, chapters, before, ...rest } = o
  return rest
}

// Same plan shape: same kinds of change to the same fields of the same types,
// whatever the wording, the values or the particular objects.
export function signature(ops, selection, objs) {
  const type = (id) => objs[id]?.type
  return [...new Set(ops.map((op) => {
    const who = selection.includes(op.id) ? `$${type(op.id)}` : op.do === 'make' ? op.type : op.id
    return `${op.do}:${who}:${Object.keys(op.fields ?? {}).sort()}:${op.field ?? ''}`
  }))].sort().join('|')
}

const FREE = [...'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789']

// Tools sit behind t, so a key only has to differ from other tools' keys. With
// none left a tool has no key, and the command bar still finds it by name.
function freeKey(name, tools) {
  const used = new Set(tools.map((t) => t.key))
  const letters = name.replace(/[^a-z0-9]/gi, '')
  return [...letters.toUpperCase(), ...letters.toLowerCase(), ...FREE].find((c) => !used.has(c))
}

// How a value came from what was there before, across every example seen.
export function explain(field, samples, x) {
  const vals = samples.map((s) => s.value)
  const was = { get: [{ var: x }, field] }
  const constant = vals.every((v) => JSON.stringify(v) === JSON.stringify(vals[0]))
  // Two runs from the same start can't tell "set to 90" from "add 90". A number
  // the request never mentions came from what was there.
  const said = samples.some((s) => String(s.run?.text ?? '').includes(String(vals[0])))
  if (constant && (typeof vals[0] !== 'number' || said)) return { ok: true, expr: typeof vals[0] === 'object' ? { lit: vals[0] } : vals[0] }
  if (vals.every((v) => typeof v === 'number')) {
    for (const m of field === 'rot' ? [360, 0] : [0, 360]) {
      for (const d of [...new Set(samples.map((s) => (s.value - (s.before ?? 0) + (m || 1e9)) % (m || 1e9)))]) {
        if (samples.every((s) => (m ? ((s.before ?? 0) + d) % m : (s.before ?? 0) + d) === s.value)) {
          const sum = { plus: [was, d] }
          return { ok: true, expr: m ? { mod: [sum, m] } : sum }
        }
      }
    }
  }
  if (constant) return { ok: true, expr: vals[0] }
  if (vals.every((v) => typeof v === 'string')) {
    const b = samples.map((s) => s.before ?? '')
    if (vals.every((v, i) => v === b[i].toLowerCase())) return { ok: true, expr: { lower: was } }
    const pre = vals[0].endsWith(b[0]) ? vals[0].slice(0, vals[0].length - b[0].length) : null
    if (pre !== null && samples.every((s) => s.value === pre + (s.before ?? ''))) return { ok: true, expr: { concat: [pre, was] } }
    const suf = vals[0].startsWith(b[0]) ? vals[0].slice(b[0].length) : null
    if (suf !== null && samples.every((s) => s.value === (s.before ?? '') + suf)) return { ok: true, expr: { concat: [was, suf] } }
  }
  return { ok: false }
}

// One word that differs between two requests is a parameter of the tool.
function slot(a, b) {
  const x = a.toLowerCase().split(/\s+/)
  const y = b.toLowerCase().split(/\s+/)
  if (x.length !== y.length) return null
  const diff = x.flatMap((w, i) => (w === y[i] ? [] : [i]))
  return diff.length === 1 ? { at: diff[0], a: x[diff[0]], b: y[diff[0]] } : null
}

// Turn two runs of the same plan into a program. Returns null when the runs
// don't explain themselves, and a model then gets to write it.
export function generalize(a, b) {
  const per = (run) => run.ops.filter((op) => run.selection.includes(op.id))
  const other = b.ops.filter((op) => !b.selection.includes(op.id))
  const param = slot(a.text, b.text)
  const steps = []
  const first = per(b).filter((op) => op.id === b.selection[0])
  if (per(b).length) {
    const inner = []
    for (const op of first) {
      if (op.do === 'trash') { inner.push({ trash: { var: 'x' } }); continue }
      if (op.do !== 'set') return null
      const fields = {}
      for (const f of Object.keys(op.fields)) {
        const samples = [a, b].flatMap((r) => per(r).filter((o) => o.do === 'set').map((o) => ({
          value: o.fields[f], before: r.before?.[o.id]?.[f], run: r,
        })))
        if (param && samples.every((s) => s.value === (s.run === a ? param.a : param.b))) { fields[f] = { var: 'word' }; continue }
        const e = explain(f, samples, 'x')
        if (!e.ok) return null
        fields[f] = e.expr
      }
      inner.push({ set: { id: { var: 'x' }, fields: { obj: fields } } })
    }
    steps.push({ for: { var: 'selection' }, as: 'x', do: inner })
  }
  for (const op of other) {
    if (op.do === 'make') steps.push({ make: { type: op.type, fields: { obj: op.fields } } })
    else if (op.do === 'set') steps.push({ set: { id: op.id, fields: { obj: op.fields } } })
    else if (op.do === 'splice') for (const add of op.add) steps.push({ add: { id: op.id, field: op.field, value: add.val } })
    else return null
  }
  return { program: steps, param: param ? param.at : null }
}

export function createAgent({ call, model }) {
  const stats = { asked: 0, viaTool: 0, viaModel: 0, cost: 0 }

  const objects = async () => {
    const s = await call('GET', '/state')
    return Object.fromEntries(s.objects.filter((o) => !o.trashed).map((o) => [o.id, flat(o)]))
  }
  const act = (body) => call('POST', '/do', body)

  async function plan(text, selection, objs) {
    const selected = selection.map((id) => objs[id]).filter(Boolean).map(brief)
    const counts = {}
    for (const o of Object.values(objs)) counts[o.type] = (counts[o.type] ?? 0) + 1
    const named = Object.values(objs).filter((o) => ['note', 'collection'].includes(o.type)).slice(0, 150).map((o) => `${o.id}: ${o.title}`)
    const prompt = `You act for the owner of a personal computer. Everything on it is an object. You change things only by answering with operations, which they review.

${LANGUAGE}

STORE: ${JSON.stringify(counts)}
NAMED:
${named.join('\n')}
SELECTED: ${JSON.stringify(selected)}
REQUEST: ${text}

Answer with one JSON object and nothing else: {"reply":"one short sentence for the owner","ops":[...]}.
Use ops only if the request asks for a change. Use only ids listed above, or new ids for "make".`
    const r = await model(prompt)
    const p = json(r.text)
    return { reply: String(p.reply ?? ''), ops: Array.isArray(p.ops) ? p.ops : [], cost: r.cost }
  }

  async function writeProgram(a, b, can) {
    const r = await model(`Write a deterministic program for a task that was done twice by hand.

${TOOL_LANGUAGE}

Example 1: selection ${JSON.stringify(a.selection)}, before ${JSON.stringify(a.before)}, produced ${JSON.stringify(a.ops)}
Example 2: selection ${JSON.stringify(b.selection)}, before ${JSON.stringify(b.before)}, produced ${JSON.stringify(b.ops)}

Answer with one JSON object and nothing else: {"program":[...]}. It must produce the same operations from the same input.`)
    stats.cost += r.cost
    return json(r.text).program
  }

  const asKinds = (ops) => ops.map((o) => o.do)

  // The second run of a plan: write it down, check it reproduces what
  // happened, and offer it. Nothing here runs until the owner approves it.
  async function learn(a, b, sig, objs, tools) {
    if (tools.some((t) => t.sig === sig)) return false
    let made = generalize(a, b)
    let wrote = 'worked out from the two runs, no model needed'
    if (!made) {
      made = { program: await writeProgram(a, b).catch(() => null), param: null }
      wrote = 'written by the model and checked against the two runs'
      if (!made.program) return false
    }
    const trial = (args) => act({ do: 'try-tool', program: made.program, can: canOf, args })
    const canOf = [...new Set(b.ops.map((o) => (o.do === 'make' ? `make:${o.type}` : o.do)))]
    const words = b.text.split(/\s+/)
    const args = { selection: b.selection, ...(made.param !== null ? { word: words[made.param].toLowerCase() } : {}) }
    const got = await trial(args).catch(() => null)
    if (!got || !sameEffect(got.result.ops, b.ops)) return false
    const name = b.text.replace(/^./, (c) => c.toUpperCase()).slice(0, 40)
    const type = objs[b.selection[0]]?.type ?? '*'
    const id = `tool:${sig.replace(/[^a-z0-9]/gi, '').slice(0, 24)}${Date.now().toString(36)}`
    const fields = {
      name, key: freeKey(name, tools), appliesTo: type, program: made.program, can: canOf, sig,
      phrases: [a.text, b.text], version: 1, uses: 0, source: wrote,
      param: made.param,
      examples: [{ args: { selection: [] }, expect: b.selection.length ? [] : asKinds(b.ops) }],
    }
    await act({ do: 'propose', actor: 'agent:host', intent: `New tool: ${name} (${fields.key ? `t ${fields.key}` : 'by name'}), ${wrote}`, ops: [{ do: 'make', id, type: 'tool', fields }] })
    return true
  }

  const sameEffect = (x, y) => {
    const key = (o) => JSON.stringify([o.do, o.do === 'make' ? o.type : o.id, Object.entries(o.fields ?? {}).sort()])
    return JSON.stringify(x.map(key).sort()) === JSON.stringify(y.map(key).sort())
  }

  async function runTool(t, selection, word) {
    const r = await act({ do: 'run-tool', id: t.id, cid: `t${Date.now().toString(36)}`, args: { selection, ...(word ? { word } : {}) } })
    await act({ do: 'batch', actor: 'agent:host', intent: 'count a tool run', ops: [{ do: 'set', id: t.id, fields: { uses: (t.uses ?? 0) + 1 } }] }).catch(() => {})
    stats.viaTool++
    return r
  }

  const tools = (objs) => Object.values(objs).filter((o) => o.type === 'tool')

  // A request that reads like a phrase a tool came from runs it directly.
  function matchTool(text, ts) {
    const words = norm(text)
    for (const t of ts) {
      for (const p of t.phrases ?? []) {
        if (t.param !== null && t.param !== undefined) {
          const x = text.toLowerCase().split(/\s+/)
          const y = p.toLowerCase().split(/\s+/)
          if (x.length === y.length && x.every((w, i) => i === t.param || w === y[i])) return { t, word: x[t.param] }
        } else if (jaccard(words, norm(p)) >= 0.75) return { t }
      }
    }
    return null
  }

  return {
    stats,
    async ask({ text, selection = [] }) {
      const started = Date.now()
      stats.asked++
      const objs = await objects()
      const ts = tools(objs)
      const hit = matchTool(text, ts)
      if (hit) {
        await runTool(hit.t, selection, hit.word)
        return { say: `Done with ${hit.t.name}, in ${Date.now() - started} ms and no model.`, via: 'tool', touched: selection }
      }
      stats.viaModel++
      const p = await plan(text, selection, objs)
      stats.cost += p.cost
      const ms = Date.now() - started
      const before = Object.fromEntries(selection.map((id) => [id, objs[id]]))
      const sig = p.ops.length ? signature(p.ops, selection, objs) : ''
      const run = { title: text, text, selection, ops: p.ops, sig, ms, cost: p.cost, before }
      await act({ do: 'batch', actor: 'agent:host', intent: `run: ${text}`, ops: [{ do: 'make', id: `run:${started.toString(36)}`, type: 'run', fields: run }] })
      if (!p.ops.length) return { say: p.reply || 'Nothing to do.', via: 'model' }
      await act({ do: 'propose', actor: 'agent:claude', intent: text, ops: p.ops })
      const earlier = Object.values(objs).filter((o) => o.type === 'run' && o.sig === sig)
      const learned = earlier.length ? await learn(earlier.at(-1), run, sig, objs, ts).catch((e) => { console.error(e); return false }) : false
      return { say: `${p.reply} It’s in the Inbox: press d.${learned ? ' That’s twice, so a tool for it is there too.' : ''}`, via: 'model', learned }
    },
    async tool({ id, selection = [] }) {
      const objs = await objects()
      const t = objs[id]
      if (!t) return { say: 'That tool is gone.' }
      await runTool(t, selection)
      return { say: `${t.name}: done, no model.`, touched: selection }
    },
  }
}
