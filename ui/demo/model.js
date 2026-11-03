// The demo's agent has no model behind it, so nobody's key pays for strangers and
// nothing typed leaves the browser. These rules stand in for one: same prompt,
// same answer shape. Everything after them (proposals, approval, learning a tool
// on the second run) is the real agent in server/agent.mjs.
const HELP = 'This demo’s agent is a short script, not a live model. Select something and try “rotate this”, “tag this blue”, “rename this to …” or “trash this”, or ask “make a note called …”, “how many photos” or “add a to-do …” on a note.'

const answer = (reply, ops = []) => ({ text: JSON.stringify({ reply, ops }), cost: 0 })
const fresh = (kind) => `${kind}-${Math.random().toString(36).slice(2, 10)}`

export async function demoModel(prompt) {
  const text = /^REQUEST: (.*)$/m.exec(prompt)?.[1] ?? ''
  const sel = JSON.parse(/^SELECTED: (.*)$/m.exec(prompt)?.[1] ?? '[]')
  const counts = JSON.parse(/^STORE: (.*)$/m.exec(prompt)?.[1] ?? '{}')
  const lower = text.toLowerCase()
  const each = (fn) => sel.map(fn)

  const count = /how many (\w+?)s?\b/.exec(lower)
  if (count) {
    const n = counts[count[1]] ?? 0
    return answer(`There ${n === 1 ? 'is' : 'are'} ${n} ${count[1]}${n === 1 ? '' : 's'} here.`)
  }
  const named = /^(?:make|create|new|start)\b.*?\bnote\b(?:\s+(?:called|named|titled|about))?\s+(.+)$/i.exec(text)
  if (named) {
    const notes = /^(\S+): Notes$/m.exec(prompt)?.[1]
    const note = fresh('note')
    const block = fresh('block')
    const title = named[1].trim()
    const ops = [
      { do: 'make', id: block, type: 'block', fields: { style: 'h', text: title } },
      { do: 'make', id: note, type: 'note', fields: { title, items: [block] } },
    ]
    if (notes) ops.push({ do: 'splice', id: notes, field: 'items', del: [], add: [{ after: null, val: note }] })
    return answer(`I’ve drafted the note “${title}”.`, ops)
  }
  if (!sel.length && /rotate|tag|rename|trash|delete|to-?do/.test(lower)) return answer('Select something first, then ask again.')
  if (/rotate|turn/.test(lower)) {
    const photos = sel.filter((o) => o.type === 'photo')
    if (!photos.length) return answer('Only photos rotate.')
    return answer('I’ll turn them a quarter.', photos.map((o) => ({ do: 'set', id: o.id, fields: { rot: ((o.rot ?? 0) + 90) % 360 } })))
  }
  const tag = /\btag\b.*\b(\w+)$/i.exec(text)
  if (tag) return answer(`I’ll tag them ${tag[1].toLowerCase()}.`, each((o) => ({ do: 'set', id: o.id, fields: { tag: tag[1].toLowerCase() } })))
  const rename = /\brename\b.*?\bto\s+(.+)$/i.exec(text)
  if (rename) return answer('I’ll rename it.', each((o) => ({ do: 'set', id: o.id, fields: { title: rename[1].trim() } })))
  if (/\b(trash|delete|remove)\b/.test(lower)) return answer('I’ll move that to the Trash.', each((o) => ({ do: 'trash', id: o.id })))
  const todo = /\badd\b.*?\bto-?do\b\s*:?\s*(.*)$/i.exec(text)
  if (todo) {
    const target = sel.find((o) => o.type === 'note')
    if (!target) return answer('Select a note first.')
    const id = fresh('block')
    return answer('I’ll add that to-do.', [
      { do: 'make', id, type: 'block', fields: { style: 'todo', text: todo[1].trim() || 'New to-do', done: false } },
      { do: 'splice', id: target.id, field: 'items', del: [], add: [{ after: target.items?.at(-1) ?? null, val: id }] },
    ])
  }
  return answer(HELP)
}
