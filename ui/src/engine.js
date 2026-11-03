// The interaction primitive, with no UI in it: navigate, select, enter, verb.
// Every key press is press(state, key) -> new state, so tests can drive it.
import { seedStore } from './data.js'
import { SLOT, addDays, dayName, dayStart, occurrences, occursIn, retime, slotOf, timeName, weekStart, whenName } from './calendar.js'

export const CELLS = { x: 12, y: 8 }
export const RESERVED = new Set([...'hjklHJKLgGsSaioxcvzZ.tfnpmwedDCM;/ '])
// These commit something that can't be taken back with a keystroke. They are
// never reused for anything else, so muscle memory can't fire one by accident.
export const CONSEQUENTIAL = new Set([...'AR'])
const NOTHING = 'Nothing selected here. Press Esc to act on this item from its list.'

let nextId = 0
const newId = () => `n${Date.now().toString(36)}${++nextId}`

const newFrame = (id, extra = {}) => ({
  id, focus: 0, marks: [], base: [], anchor: null, head: null, extending: false,
  filter: '', cursor: 0, cell: { x: 6, y: 4 }, view: null, ...extra,
})

export function init(store = seedStore(), now = Date.now() / 1000) {
  return {
    store,
    now,
    layout: { pane: 'p1' },
    panes: { p1: { stack: [newFrame('home')] } },
    cols: { p1: 4 },
    active: 'p1',
    paneSeq: 1,
    mode: 'normal',
    menuPath: [],
    bar: { text: '', i: 0 },
    typing: null,
    held: null,
    undo: [],
    redo: [],
    last: null,
    player: { id: null, t: 0, playing: false, volume: 70, queue: [] },
    msg: '',
    out: [],
    working: null,
    history: [],
    handled: false,
  }
}

// ---------- Reading state ----------

export const frame = (s, pane = s.active) => s.panes[pane].stack.at(-1)

export function kind(s, f) {
  if (f.view === 'info') return 'info'
  if (f.view === 'history') return 'history'
  const o = s.store[f.id]
  if (o.type === 'collection') return o.layout
  return { note: 'blocks', event: 'blocks', block: 'chars', photo: 'photo', track: 'player', movie: 'player', proposal: 'proposal', tool: 'tool', thread: 'thread' }[o.type]
}

export function titleOf(s, id) {
  const o = s.store[id]
  if (!o) return 'Missing'
  if (o.type === 'block') return o.ref ? titleOf(s, o.ref) : o.text
  if (o.type === 'message') return o.text
  return o.title ?? o.name ?? o.intent ?? o.id
}

function backlinks(s, id) {
  const out = []
  for (const o of Object.values(s.store)) {
    if ((o.type === 'note' || o.type === 'event') && o.items?.some((b) => s.store[b]?.ref === id)) out.push(o.id)
    else if (o.type === 'collection' && o.items?.includes(id)) out.push(o.id)
  }
  return out
}

export function items(s, f) {
  if (f.view === 'info') return backlinks(s, f.id)
  if (f.view === 'history') return s.history.map((e) => e.seq)
  const o = s.store[f.id]
  const list = o.query ? matching(s, o.query) : o.items
  if (!list) return []
  const q = f.filter.toLowerCase()
  return q ? list.filter((id) => titleOf(s, id).toLowerCase().includes(q)) : list
}

// A collection with a query is a View: derived, never stored.
function matching(s, q) {
  return Object.values(s.store)
    .filter((o) => !!o.$trashed === !!q.$trashed && Object.entries(q).every(([k, v]) => o[k] === v))
    .sort((a, b) => (b.$at ?? 0) - (a.$at ?? 0))
    .map((o) => o.id)
}

const isList = (k) => ['grid', 'list', 'blocks', 'info', 'history', 'thread'].includes(k)
// Views where verbs act on chosen objects. A week isn't a list: its cursor is a
// time, and what it picks comes from there.
const picks = (k) => isList(k) || k === 'calendar'
export const queryItems = matching
const pos = (s, f, k) => (k === 'calendar' ? f.at : k === 'chars' ? f.cursor : k === 'photo' ? { ...f.cell } : k === 'player' ? playerOf(s, f).t : f.focus)
export const playerOf = (s, f) => (s.player.id === f.id ? s.player : { ...s.player, t: 0, playing: false })

// The current range in a frame's own units, or null.
export function range(s, f) {
  if (f.anchor === null) return null
  const k = kind(s, f)
  const b = f.extending ? pos(s, f, k) : f.head
  if (k === 'photo') {
    return {
      x: Math.min(f.anchor.x, b.x), y: Math.min(f.anchor.y, b.y),
      w: Math.abs(f.anchor.x - b.x) + 1, h: Math.abs(f.anchor.y - b.y) + 1,
    }
  }
  const lo = Math.min(f.anchor, b)
  const hi = Math.max(f.anchor, b)
  if (k === 'calendar') return { from: lo, to: hi + SLOT }
  return k === 'chars' ? { from: lo, to: hi + 1 } : { from: lo, to: hi }
}

// What local verbs will hit: marks if any, otherwise the focused item.
function targets(s, f) {
  if (kind(s, f) === 'calendar') {
    if (f.marks.length) return f.marks.filter((id) => s.store[f.id].items.includes(id))
    const id = underCursor(s, f)?.id
    return id ? [id] : []
  }
  const list = items(s, f)
  if (f.marks.length) return f.marks.filter((id) => list.includes(id))
  return list[f.focus] ? [list[f.focus]] : []
}

const NOUN = { event: 'event', photo: 'photo', note: 'note', track: 'track', movie: 'movie', block: 'block', collection: 'item', message: 'message', thread: 'thread' }
const plural = (n, w) => `${n} ${w}${n === 1 ? '' : 's'}`

export function targetLabel(s) {
  const f = frame(s)
  const k = kind(s, f)
  const r = range(s, f)
  if (k === 'history') return `Change ${items(s, f)[f.focus] ?? ''}`
  if (k === 'chars' && r) return plural(r.to - r.from, 'character')
  if (k === 'photo' && r) return `Part of ${titleOf(s, f.id)}`
  if (k === 'calendar') {
    if (r) return whenName(r.from, r.to)
    const t = targets(s, f)
    if (t.length > 1) return plural(t.length, 'event')
    return t.length ? titleOf(s, t[0]) || 'Untitled' : `${dayName(f.at)}, ${timeName(f.at)}`
  }
  if (k === 'player' && r) return `${clock(r.from)} to ${clock(r.to)} of ${titleOf(s, f.id)}`
  if (isList(k)) {
    const t = targets(s, f)
    if (!t.length) return 'Nothing here'
    if (t.length > 1) {
      const types = new Set(t.map((id) => s.store[id].type))
      return plural(t.length, types.size === 1 ? NOUN[[...types][0]] : 'item')
    }
    return titleOf(s, t[0])
  }
  return titleOf(s, f.id)
}

export function clock(t) {
  const x = Math.max(0, Math.floor(t))
  const h = Math.floor(x / 3600)
  const m = Math.floor(x / 60) % 60
  const ss = String(x % 60).padStart(2, '0')
  return h ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`
}

// A verb belongs to a type, and acts on objects of that type within reach.
function typeTargets(s, type) {
  if (frame(s).view === 'history') return []
  const f = frame(s)
  const k = kind(s, f)
  if (s.store[f.id].type === type && !isList(k)) return [f.id]
  if (!picks(k)) return []
  const ids = targets(s, f).map((id) => s.store[id].ref ?? id)
  return ids.filter((id) => s.store[id].type === type)
}

// ---------- Menu and commands ----------

export function rootMenu(s) {
  const f = frame(s)
  const k = kind(s, f)
  const out = []
  if (k === 'calendar') {
    out.push({ key: 'u', label: 'Today', run: 'today' })
    if (targets(s, f).length) {
      out.push({ key: '<', label: 'Shorten', run: 'endLess' }, { key: '>', label: 'Lengthen', run: 'endMore' }, { key: 'y', label: 'Repeat', run: 'repeat' })
    }
  }
  const photos = typeTargets(s, 'photo')
  if (photos.length) {
    out.push({ key: 'r', label: 'Rotate', run: 'rotate' })
    if (k === 'photo') out.push({ key: 'X', label: 'Crop', run: 'crop', off: !range(s, f) })
    if (photos.some((id) => s.store[id].crop)) out.push({ key: 'u', label: 'Remove crop', run: 'uncrop' })
  }
  if (k === 'blocks' && targets(s, f).some((id) => !s.store[id].ref)) {
    out.push({ key: 'y', label: 'To-do', run: 'kindT' }, { key: 'b', label: 'Heading', run: 'kindH' }, { key: 'q', label: 'Paragraph', run: 'kindP' })
  }
  if ((k === 'list' || k === 'grid') && !s.store[f.id].fixed) {
    out.push({ key: 'y', label: 'Sort by name', run: 'sortName' }, { key: 'Y', label: 'Reverse order', run: 'sortReverse' })
  }
  if (k === 'proposal' && s.store[f.id].status === 'open') {
    out.push({ key: 'A', label: 'Approve', run: 'approve' }, { key: 'R', label: 'Reject', run: 'reject' })
  }
  if (k === 'thread') {
    const m = messageTarget(s, f)
    if (m) out.push({ key: 'A', label: m.status === 'failed' ? 'Send again' : 'Send', run: 'send' })
    out.push({ key: 'P', label: 'Set ship', run: 'setPeer' })
  }
  if (k === 'history' && s.history.length) out.push({ key: 'u', label: 'Undo this change', run: 'revert' })
  if (s.store[f.id]?.id === 'trash' && k === 'list' && items(s, f).length) out.push({ key: 'u', label: 'Restore', run: 'restore' })
  if (s.store[f.id]?.$clashes?.length) {
    out.push({ key: 'O', label: 'Keep the other version', run: 'keepTheirs' }, { key: 'V', label: 'Keep this version', run: 'keepMine' })
  }
  const tools = toolEntries(s)
  if (tools.length) out.push({ key: 't', label: 'Tools', children: tools })
  out.push({ key: 'I', label: 'Info', run: 'info' })
  out.push({ key: 'T', label: 'Open in a new tile', run: 'newTile' })
  return out
}

// Tools the agent has learned live behind t so they can never take a key from
// the interface, and there is room for as many as get learned. Two tools that
// apply here may want one key: the first by id keeps it, the rest take another,
// and past the last free key a tool is still reachable by name in the bar.
const TOOL_KEYS = [...'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789']

function toolEntries(s) {
  const f = frame(s)
  const k = kind(s, f)
  const here = k === 'photo' || k === 'chars' || k === 'player' || k === 'blocks' ? s.store[f.id]?.type : null
  const ids = picks(k) ? targets(s, f) : [f.id]
  const types = new Set([here, ...ids.map((id) => s.store[id]?.type)].filter(Boolean))
  const tools = Object.values(s.store)
    .filter((o) => o.type === 'tool' && !o.$trashed && (o.appliesTo === '*' || types.has(o.appliesTo)))
    .sort((a, b) => (a.id < b.id ? -1 : 1))
  const keys = new Map()
  const used = new Set()
  for (const o of tools) if (o.key && !used.has(o.key)) { keys.set(o.id, o.key); used.add(o.key) }
  for (const o of tools) {
    if (keys.has(o.id)) continue
    const letters = o.name.replace(/[^a-z0-9]/gi, '')
    const key = [...letters.toUpperCase(), ...letters.toLowerCase(), ...TOOL_KEYS].find((c) => !used.has(c))
    if (key) { keys.set(o.id, key); used.add(key) }
  }
  return tools.map((o) => ({ key: keys.get(o.id), label: o.name, run: `tool:${o.id}` }))
}

// What this view can do, with the keys that do it. Menu actions come straight
// from rootMenu, so a label can't disagree with what the key runs.
export function viewActions(s) {
  const f = frame(s)
  const k = kind(s, f)
  const o = s.store[f.id]
  const on = {
    calendar: targets(s, f).length
      ? [['Space', 'Open'], ['i', 'Rename'], ['s', 'Select']]
      : [['o', 'New'], ['S', range(s, f) ? 'Finish time' : 'Select time']],
    grid: [['Space', 'Open'], ['s', 'Select'], ['o', 'New']],
    list: [['Space', 'Open'], ['s', 'Select'], ['o', 'New']],
    blocks: [['Space', 'Open'], ['i', 'Edit'], ['o', 'New line']],
    thread: [['o', 'Write'], ['s', 'Select'], ['c', 'Copy']],
    chars: [['S', 'Select'], ['i', 'Type']],
    photo: f.extending
      ? [['h j k l', 'Resize part'], ['S', 'Finish part']]
      : [['h l', 'Previous, next'], ['S', 'Select part']],
  }[k] ?? []
  if (k === 'player') {
    const big = o.type === 'movie' ? 'Chapter' : 'Track'
    on.push(['Space', playerOf(s, f).playing ? 'Pause' : 'Play'],
      ['h l', 'Seek'], ['H L', big], ['j k', 'Volume'], ['S', o.type === 'movie' ? 'Mark clip' : 'Mark part'])
  }
  const menu = rootMenu(s).filter((e) => !['I', 'T'].includes(e.key)).map((e) => [e.key, e.label, e.off])
  return [...on, ...menu]
}

export function menuEntries(s) {
  let m = rootMenu(s)
  for (const key of s.menuPath) m = m.find((e) => e.key === key)?.children ?? []
  return m
}

function leaves(m) {
  return m.flatMap((e) => (e.children ? leaves(e.children).map((c) => ({ ...c, label: `${e.label}: ${c.label}` })) : [e]))
}

export function completions(s) {
  const q = s.bar.text.trim().toLowerCase()
  const commands = [
    ...leaves(rootMenu(s)).filter((e) => !e.off),
    { label: 'Go home', run: 'home' },
    { label: 'Close tile', run: 'closeTile' },
    { label: 'Undo', run: 'undo' },
    { label: 'Redo', run: 'redo' },
    { label: 'History', run: 'history' },
    { label: 'Trash', run: 'trash' },
    { label: 'Inbox', run: 'inbox' },
    { label: `Command bar: ${pref(s, 'bar') === 'line' ? 'floating' : 'in the top line'}`, run: 'toggleBar' },
    { label: `Notices: ${pref(s, 'notices') === 'line' ? 'toasts' : 'in the top line'}`, run: 'toggleNotices' },
  ]
  const opens = Object.values(s.store)
    .filter((o) => o.title && o.id !== 'home' && !o.$trashed)
    .map((o) => ({ label: `Open ${o.title}`, name: o.title, open: o.id }))
  if (!q) return commands.slice(0, 6)
  const hit = [...commands, ...opens].filter((c) => (c.name ?? c.label).toLowerCase().includes(q))
  const score = (c) => ((c.name ?? c.label).toLowerCase().startsWith(q) ? 0 : 1)
  return hit.sort((a, b) => score(a) - score(b)).slice(0, 6)
}

// ---------- Changing objects ----------

const dirty = (s, label) => { s._dirty = label }

// Both versions of an open design question stay built. Prefs sync like any
// other object, so a choice follows you across devices.
const PREFS = { bar: 'line', notices: 'line' }
export const pref = (s, k) => s.store.prefs?.[k] ?? PREFS[k]
function setPref(s, k, v) {
  s.store.prefs = { ...(s.store.prefs ?? { id: 'prefs', type: 'prefs' }), [k]: v }
}

const VERBS = {
  toggleBar(s) {
    setPref(s, 'bar', pref(s, 'bar') === 'line' ? 'float' : 'line')
    return `The command bar is now ${pref(s, 'bar') === 'line' ? 'in the top line' : 'floating'}.`
  },
  toggleNotices(s) {
    setPref(s, 'notices', pref(s, 'notices') === 'line' ? 'toast' : 'line')
    return `Notices are now ${pref(s, 'notices') === 'line' ? 'in the top line' : 'toasts'}.`
  },
  history(s) {
    s.out.push({ do: 'history' })
    s.panes[s.active].stack.push(newFrame(frame(s).id, { view: 'history' }))
    return ''
  },
  trash(s) { open(s, 'trash'); return '' },
  inbox(s) { launch(s, 'd'); return '' },
  revert(s) {
    const f = frame(s)
    s.out.push({ do: 'revert', seq: s.history[f.focus].seq })
    return 'Undoing that change.'
  },
  restore(s) {
    targets(s, frame(s)).forEach((id) => s.out.push({ do: 'restore', id }))
    return 'Restored.'
  },
  keepTheirs(s) {
    const f = frame(s)
    s.store[f.id].$clashes.forEach((c) => s.out.push({ do: 'resolve', id: f.id, field: c.field, keep: true }))
    return 'Kept the other version.'
  },
  keepMine(s) {
    const f = frame(s)
    s.store[f.id].$clashes.forEach((c) => s.out.push({ do: 'resolve', id: f.id, field: c.field, keep: false }))
    return 'Kept this version.'
  },
  approve(s) {
    s.out.push({ do: 'decide', id: frame(s).id, ok: true })
    return 'Approved.'
  },
  reject(s) {
    s.out.push({ do: 'decide', id: frame(s).id, ok: false })
    return 'Rejected.'
  },
  today(s) {
    const f = frame(s)
    f.at = slotOf(s.now)
    f.sel = null
    return ''
  },
  endLess: (s) => resize(s, -SLOT),
  endMore: (s) => resize(s, SLOT),
  repeat(s) {
    const ids = targets(s, frame(s))
    dirty(s, 'repeat')
    const next = { daily: 'weekly', weekly: null }[s.store[ids[0]].repeat] ?? (s.store[ids[0]].repeat ? null : 'daily')
    for (const id of ids) s.store[id].repeat = next
    return next ? `Repeats ${next}.` : 'Doesn’t repeat.'
  },
  send(s) {
    const f = frame(s)
    const m = messageTarget(s, f)
    if (!m) return 'Nothing to send. Press o to write a message.'
    const t = s.store[f.id]
    if (!validPeer(t.peer)) return 'This thread has no ship to send to. Press P and type one, like ~sampel-palnet.'
    if (new TextEncoder().encode(m.text).length > MAX_MESSAGE) return `That message is over the ${MAX_MESSAGE} byte limit.`
    // Not an edit, so it never enters undo: what has left can't be taken back.
    m.$sending = true
    s.out.push({ do: 'send', id: m.id })
    return `Sending to ${t.peer}…`
  },
  setPeer(s) {
    startTyping(s, frame(s).id, 'peer')
    s.typing.note = 'Type their ship, like ~sampel-palnet. Enter to finish.'
    return ''
  },
  cut(s) {
    const f = frame(s)
    const k = kind(s, f)
    if (k === 'calendar') {
      const ids = targets(s, f)
      if (!ids.length) return 'Nothing to cut here.'
      const starts = heldStarts(s, f, ids)
      const label = labelFor(s, ids)
      const series = ids.some((id) => s.store[id].repeat)
      dirty(s, `cut ${label}`)
      s.store[f.id].items = s.store[f.id].items.filter((id) => !ids.includes(id))
      s.held = { kind: 'items', ids, cut: true, label, starts }
      clearRange(f)
      f.marks = []
      return `Cut ${label}${series ? ', every repeat' : ''}. Press v to paste, or z to undo.`
    }
    if (k === 'thread' && targets(s, f).some((id) => s.store[id].status !== 'draft')) return 'A sent message can’t be unsent.'
    if (k === 'chars') {
      const o = s.store[f.id]
      const r = range(s, f) ?? { from: f.cursor, to: f.cursor + 1 }
      const text = o.text.slice(r.from, r.to)
      if (!text) return 'Nothing to cut here.'
      dirty(s, 'cut text')
      o.text = o.text.slice(0, r.from) + o.text.slice(r.to)
      s.held = { kind: 'text', text, label: `“${text.slice(0, 24)}”` }
      f.cursor = r.from
      clearRange(f)
      return 'Cut. Press v to paste.'
    }
    if (!isList(k) || k === 'info') return NOTHING
    const o = s.store[f.id]
    if (o.fixed) return 'Home can’t be changed.'
    const ids = targets(s, f)
    if (!ids.length) return 'Nothing to cut here.'
    dirty(s, `cut ${targetLabel(s)}`)
    o.items = o.items.filter((id) => !ids.includes(id))
    s.held = { kind: 'items', ids, cut: true, label: labelFor(s, ids) }
    clearRange(f)
    f.marks = []
    return `Cut ${s.held.label}. Press v to paste, or z to undo.`
  },
  copy(s) {
    const f = frame(s)
    const k = kind(s, f)
    if (k === 'chars') {
      const r = range(s, f)
      const text = r ? s.store[f.id].text.slice(r.from, r.to) : s.store[f.id].text
      s.held = { kind: 'text', text, label: `“${text.slice(0, 24)}”` }
      clearRange(f)
    } else if (k === 'photo') {
      const r = range(s, f)
      if (!r) return NOTHING
      s.held = { kind: 'region', id: f.id, crop: regionToBase(s.store[f.id], r), label: `part of ${titleOf(s, f.id)}` }
      clearRange(f)
    } else if (k === 'player') {
      const r = range(s, f)
      if (!r) return NOTHING
      s.held = { kind: 'clip', id: f.id, from: r.from, to: r.to, label: `a clip of ${titleOf(s, f.id)}` }
      clearRange(f)
    } else if (k === 'calendar') {
      const ids = targets(s, f)
      if (!ids.length) return 'Nothing to copy here.'
      s.held = { kind: 'items', ids, cut: false, label: labelFor(s, ids), starts: heldStarts(s, f, ids) }
      f.marks = []
      clearRange(f)
    } else {
      const ids = targets(s, f)
      if (!ids.length) return 'Nothing to copy here.'
      s.held = { kind: 'items', ids, cut: false, label: labelFor(s, ids) }
      f.marks = []
      clearRange(f)
    }
    return `Copied ${s.held.label}.`
  },
  paste(s) {
    const f = frame(s)
    const k = kind(s, f)
    const h = s.held
    if (!h) return 'Nothing to paste. Copy something with c first.'
    if (k === 'calendar') return pasteTimed(s, f, h)
    if (k === 'thread') {
      const m = ensureDraft(s, f)
      const words = h.kind === 'text' ? h.text : h.kind === 'items' ? h.ids.map((id) => titleOf(s, id)).join(', ') : h.label
      dirty(s, 'paste')
      s.store[m.id] = { ...m, text: m.text + (m.text && !m.text.endsWith(' ') ? ' ' : '') + words }
      f.focus = items(s, f).indexOf(m.id)
      return 'Pasted into your draft. Messages carry text only.'
    }
    if (k === 'chars') {
      const text = h.kind === 'text' ? h.text : h.label
      const o = s.store[f.id]
      dirty(s, 'paste')
      o.text = o.text.slice(0, f.cursor) + text + o.text.slice(f.cursor)
      f.cursor += text.length
      return 'Pasted.'
    }
    if (!isList(k) || k === 'info') return 'Can’t paste here. Go to a list or a note first.'
    const o = s.store[f.id]
    if (o.fixed) return 'Home can’t be changed.'
    const list = items(s, f)
    const after = list[f.focus]
    const at = after ? o.items.indexOf(after) + 1 : o.items.length
    let add = []
    if (k === 'blocks') {
      if (h.kind === 'text') add = [block(s, { style: 'p', text: h.text })]
      else if (h.kind === 'clip') add = [block(s, { ref: h.id, clip: { from: h.from, to: h.to } })]
      else if (h.kind === 'region') add = [block(s, { ref: h.id, crop: h.crop })]
      else {
        add = h.ids.map((id) => {
          const src = s.store[id]
          if (src.type !== 'block') return block(s, { ref: id })
          return h.cut ? id : block(s, { ...src, id: undefined })
        })
      }
    } else {
      if (h.kind !== 'items') return `${o.title} can’t hold that.`
      const fits = h.ids.filter((id) => s.store[id].type === o.accepts && !o.items.includes(id))
      if (!fits.length) return `${o.title} can’t hold ${h.label}.`
      add = fits
    }
    dirty(s, 'paste')
    o.items.splice(at, 0, ...add)
    if (after) f.focus += 1
    if (h.kind === 'items' && h.cut) s.held = { ...h, cut: false }
    return `Pasted ${h.label}.`
  },
  rotate(s) {
    const ids = typeTargets(s, 'photo')
    if (!ids.length) return 'Rotate works on photos.'
    dirty(s, 'rotate')
    for (const id of ids) s.store[id].rot = (s.store[id].rot + 90) % 360
    const f = frame(s)
    const all = isList(kind(s, f)) ? targets(s, f).length : 1
    return ids.length === all ? `Rotated ${plural(ids.length, 'photo')}.` : `Rotated ${ids.length} of ${all}.`
  },
  crop(s) {
    const f = frame(s)
    const r = range(s, f)
    if (kind(s, f) !== 'photo' || !r) return 'Mark a region first: press S, then move.'
    dirty(s, 'crop')
    const o = s.store[f.id]
    o.crop = regionToBase(o, r)
    clearRange(f)
    return 'Cropped.'
  },
  uncrop(s) {
    const ids = typeTargets(s, 'photo')
    dirty(s, 'remove crop')
    for (const id of ids) s.store[id].crop = null
    return 'Crop removed.'
  },
  kindP: (s) => setKind(s, 'p'),
  kindH: (s) => setKind(s, 'h'),
  kindT: (s) => setKind(s, 'todo'),
  sortName(s) {
    const f = frame(s)
    dirty(s, 'sort')
    s.store[f.id].items.sort((a, b) => titleOf(s, a).localeCompare(titleOf(s, b)))
    return 'Sorted by name.'
  },
  sortReverse(s) {
    const f = frame(s)
    dirty(s, 'reverse')
    s.store[f.id].items.reverse()
    return 'Reversed.'
  },
  info(s) {
    const f = frame(s)
    const k = kind(s, f)
    const t = k === 'calendar' ? targets(s, f)[0] ?? f.id : isList(k) ? targets(s, f).map((id) => s.store[id].ref ?? id)[0] : f.id
    if (!t) return 'Nothing to look at here.'
    s.panes[s.active].stack.push(newFrame(t, { view: 'info' }))
    return ''
  },
  newTile,
  closeTile,
  home(s) {
    s.panes[s.active].stack.splice(1)
    return ''
  },
  undo,
  redo,
}

const REPEATABLE = new Set(['endLess', 'endMore', 'cut', 'copy', 'paste', 'rotate', 'kindP', 'kindH', 'kindT', 'uncrop'])

function runVerb(s, name) {
  if (name.startsWith('tool:')) {
    const id = name.slice(5)
    s.out.push({ do: 'tool', id, selection: selected(s) })
    s.msg = `Running ${s.store[id].name}…`
    return
  }
  s.msg = VERBS[name](s) ?? ''
  if (REPEATABLE.has(name)) s.last = name
}

// %messages takes up to this many bytes in one message.
export const MAX_MESSAGE = 8192
export const validPeer = (p) => /^~(?:[a-z]{3}|[a-z]{6})(?:-[a-z]{6})*(?:--[a-z]{6}(?:-[a-z]{6})*)?$/.test(p ?? '')

export const draftOf = (s, t) => (t.items ?? []).map((id) => s.store[id]).find((m) => m?.status === 'draft')

// What A would send here: the failed message under the cursor, or the draft.
function messageTarget(s, f) {
  const here = s.store[items(s, f)[f.focus]]
  if (here?.status === 'failed' && !here.$sending) return here
  const d = draftOf(s, s.store[f.id])
  return d && d.text.trim() && !d.$sending ? d : null
}

function ensureDraft(s, f) {
  const t = s.store[f.id]
  const have = draftOf(s, t)
  if (have) return have
  dirty(s, 'new message')
  const id = newId()
  s.store[id] = { id, type: 'message', text: '', from: 'me', status: 'draft' }
  t.items.push(id)
  return s.store[id]
}

function compose(s, f) {
  const fresh = !draftOf(s, s.store[f.id])
  const m = ensureDraft(s, f)
  f.filter = ''
  f.focus = items(s, f).indexOf(m.id)
  startTyping(s, m.id, 'text')
  s.typing.endsBlock = false
  s.typing.dirtied = fresh
}

// An empty draft is nothing, so it goes when you stop typing.
function dropEmptyDraft(s, id) {
  const m = s.store[id]
  if (m?.type !== 'message' || m.status !== 'draft' || m.text.trim()) return
  const t = Object.values(s.store).find((o) => o.type === 'thread' && o.items.includes(id))
  if (t) t.items = t.items.filter((x) => x !== id)
  delete s.store[id]
}

// Undo can't reach what has left this device or arrived from elsewhere: those
// messages, and their place in a thread, stay as they are now.
function keepDelivered(cur, next) {
  const left = (o) => o?.type === 'message' && (o.status !== 'draft' || o.$sending)
  for (const id in cur) if (left(cur[id])) next[id] = cur[id]
  for (const id in cur) {
    if (cur[id].type !== 'thread' || !next[id]) continue
    const kept = cur[id].items.filter((x) => next[x])
    next[id] = { ...next[id], items: [...kept, ...next[id].items.filter((x) => !kept.includes(x))] }
  }
  return next
}

function labelFor(s, ids) {
  if (ids.length === 1) return titleOf(s, ids[0])
  const types = new Set(ids.map((id) => s.store[id].type))
  return plural(ids.length, types.size === 1 ? NOUN[[...types][0]] : 'item')
}

function block(s, fields) {
  const id = newId()
  s.store[id] = { type: 'block', ...fields, id }
  return id
}

function setKind(s, style) {
  const f = frame(s)
  const ids = targets(s, f).filter((id) => !s.store[id].ref)
  dirty(s, 'change kind')
  for (const id of ids) {
    s.store[id].style = style
    if (style === 'todo') s.store[id].done ??= false
  }
  return ''
}

// ---------- Calendar ----------

const calendarEvents = (s, f) => s.store[f.id].items.map((id) => s.store[id]).filter((o) => o?.type === 'event')

// The event under the cursor. sel tells apart two that share a slot.
function underCursor(s, f) {
  const hits = occurrences(calendarEvents(s, f), f.at, f.at + SLOT)
  return hits.find((h) => h.id === f.sel) ?? hits[0] ?? null
}

// Where a repeating event was picked up, so it lands relative to that repeat.
function heldStarts(s, f, ids) {
  const here = underCursor(s, f)
  const week = weekStart(f.at)
  const out = {}
  for (const id of ids) {
    const o = s.store[id]
    out[id] = (here?.id === id ? here.start : occursIn(o, week, addDays(week, 7))[0]?.start) ?? o.start
  }
  return out
}

const overlapNote = (s, f, id) => {
  const o = s.store[id]
  const other = occurrences(calendarEvents(s, f).filter((x) => x.id !== id), o.start, o.end)[0]
  return other ? ` Overlaps ${titleOf(s, other.id) || 'Untitled'}.` : ''
}

function newEvent(s, f, fields) {
  const id = newId()
  s.store[id] = { id, type: 'event', title: '', items: [], ...fields }
  s.store[f.id].items.push(id)
  f.sel = id
  return id
}

const copyBlocks = (s, ids) => ids.map((id) => block(s, { ...s.store[id], id: undefined }))

function pasteTimed(s, f, h) {
  const r = range(s, f)
  const start = r?.from ?? f.at
  const end = r?.to ?? start + 3600
  const place = (fields) => {
    dirty(s, 'paste')
    const id = newEvent(s, f, { start, end, ...fields })
    clearRange(f)
    return `Pasted ${h.label}.${overlapNote(s, f, id)}`
  }
  if (h.kind === 'text') return place({ title: h.text })
  if (h.kind !== 'items') return 'The calendar can’t hold that.'
  const events = h.ids.filter((id) => s.store[id].type === 'event')
  if (!events.length) {
    const made = h.ids.map((id) => (s.store[id].type === 'block' && !s.store[id].ref ? copyBlocks(s, [id])[0] : block(s, { ref: s.store[id].ref ?? id })))
    return place({ title: h.label, items: made })
  }
  dirty(s, 'paste')
  const starts = events.map((id) => h.starts?.[id] ?? s.store[id].start)
  const base = Math.min(...starts)
  const placed = events.map((id, i) => {
    const o = s.store[id]
    const first = retime(o.start, base, start)
    const moved = { start: first, end: first + o.end - o.start }
    if (h.cut) {
      Object.assign(o, moved)
      s.store[f.id].items.push(id)
      f.sel = id
      return id
    }
    const { id: _, $rev, $at, $clashes, ...rest } = o
    return newEvent(s, f, { ...rest, ...moved, items: copyBlocks(s, o.items) })
  })
  if (h.cut) s.held = { ...h, cut: false }
  clearRange(f)
  const series = events.some((id) => s.store[id].repeat)
  return `Pasted ${h.label}${series ? ', every repeat' : ''}.${overlapNote(s, f, placed[0])}`
}

function resize(s, by) {
  const ids = targets(s, frame(s))
  if (!ids.length) return NOTHING
  dirty(s, 'change length')
  for (const id of ids) {
    const o = s.store[id]
    o.end = Math.max(o.start + SLOT, o.end + by)
  }
  return ids.some((id) => s.store[id].repeat) ? 'Changed every repeat.' : ''
}

// Next or previous occurrence, one at a time even when they share a start.
function hop(s, f, dir) {
  const shown = new Set(items(s, f))
  const all = occurrences(calendarEvents(s, f).filter((o) => shown.has(o.id)), addDays(f.at, -400), addDays(f.at, 400))
  const here = underCursor(s, f)
  const at = here && shown.has(here.id) ? here : { start: f.at, id: dir > 0 ? '' : '\uffff' }
  const later = (o) => o.start > at.start || (o.start === at.start && o.id > at.id)
  const to = dir > 0 ? all.find(later) : all.findLast((o) => !later(o) && !(o.start === at.start && o.id === at.id))
  if (!to) { s.msg = 'No more events.'; return }
  f.at = slotOf(to.start)
  f.sel = to.id
}

function moveCalendar(s, f, key) {
  if (key === 'J') return hop(s, f, 1)
  if (key === 'K') return hop(s, f, -1)
  f.sel = null
  if (key === 'j' || key === 'k') {
    const to = f.at + (key === 'j' ? SLOT : -SLOT)
    if (dayStart(to) === dayStart(f.at)) f.at = to
    return
  }
  const week = weekStart(f.at)
  const to = {
    h: addDays(f.at, -1), l: addDays(f.at, 1), H: addDays(f.at, -7), L: addDays(f.at, 7),
    g: retime(f.at, f.at, week) + f.at - dayStart(f.at),
    G: retime(f.at, f.at, addDays(week, 6)) + f.at - dayStart(f.at),
  }[key]
  if (to !== undefined) f.at = to
}

// Pointing at the week: a slot, and the event there if there is one.
export function pointAt(state, pane, at, id = null) {
  const s = { ...state, active: pane, panes: structuredClone(state.panes) }
  const f = frame(s, pane)
  f.at = at
  f.sel = id
  return s
}

function clearRange(f) {
  f.anchor = null
  f.head = null
  f.extending = false
}

// Display position inside a photo, mapped back to the unrotated picture.
export const BASE = { w: 1200, h: 800 }
export const visibleCrop = (o) => o.crop ?? { x: 0, y: 0, w: BASE.w, h: BASE.h }
// Crops live in BASE units whatever the picture's real shape, so this gives
// the shape actually on screen, after any rotation.
export function visibleSize(o, c = visibleCrop(o)) {
  const w = c.w * (o.w ?? BASE.w) / BASE.w
  const h = c.h * (o.h ?? BASE.h) / BASE.h
  return o.rot % 180 ? { w: h, h: w } : { w, h }
}

function regionToBase(o, r) {
  const c = visibleCrop(o)
  const corner = (u, v) => {
    const [a, b] = {
      0: [u, v], 90: [v, 1 - u], 180: [1 - u, 1 - v], 270: [1 - v, u],
    }[o.rot]
    return [c.x + a * c.w, c.y + b * c.h]
  }
  const [x1, y1] = corner(r.x / CELLS.x, r.y / CELLS.y)
  const [x2, y2] = corner((r.x + r.w) / CELLS.x, (r.y + r.h) / CELLS.y)
  return { x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) }
}

function undo(s) {
  const e = s.undo.at(-1)
  if (!e) return 'Nothing to undo.'
  s.undo = s.undo.slice(0, -1)
  s.redo = [...s.redo, { store: s.store, label: e.label }]
  s.store = keepDelivered(s.store, copyStore(e.store))
  return `Undid ${e.label}.`
}

function redo(s) {
  const e = s.redo.at(-1)
  if (!e) return 'Nothing to redo.'
  s.redo = s.redo.slice(0, -1)
  s.undo = [...s.undo, { store: s.store, label: e.label }]
  s.store = keepDelivered(s.store, copyStore(e.store))
  return `Redid ${e.label}.`
}

// ---------- Moving ----------

function setFocus(s, f, k, i) {
  const n = items(s, f).length
  f.focus = Math.max(0, Math.min(n - 1, i))
  extendList(s, f)
}

function extendList(s, f) {
  if (!f.extending) return
  const list = items(s, f)
  const lo = Math.min(f.anchor, f.focus)
  const hi = Math.max(f.anchor, f.focus)
  f.marks = [...new Set([...f.base, ...list.slice(lo, hi + 1)])]
}

function nextHeading(s, f, dir) {
  const list = items(s, f)
  for (let i = f.focus + dir; i >= 0 && i < list.length; i += dir) {
    if (s.store[list[i]].style === 'h') return i
  }
  return dir > 0 ? list.length - 1 : 0
}

function wordStep(text, i, dir) {
  let p = i
  if (dir > 0) {
    while (p < text.length && text[p] !== ' ') p++
    while (p < text.length && text[p] === ' ') p++
  } else {
    p = Math.max(0, p - 1)
    while (p > 0 && text[p] === ' ') p--
    while (p > 0 && text[p - 1] !== ' ') p--
  }
  return p
}

const sentenceStep = (text, i, dir) => {
  const stops = [...text.matchAll(/[.!?]\s+/g)].map((m) => m.index + m[0].length)
  const all = [0, ...stops, text.length]
  return dir > 0 ? (all.find((x) => x > i) ?? text.length) : (all.findLast((x) => x < i) ?? 0)
}

function seek(s, f, to) {
  if (s.player.id !== f.id) return
  s.player.t = Math.max(0, Math.min(s.store[f.id].dur, to))
}

function move(s, key) {
  const f = frame(s)
  const k = kind(s, f)
  if (k === 'calendar') return moveCalendar(s, f, key)
  const n = items(s, f).length
  const cols = s.cols[s.active] ?? 4

  if (k === 'grid') {
    const row = f.focus - (f.focus % cols)
    const to = {
      h: f.focus - 1, l: f.focus + 1, j: f.focus + cols, k: f.focus - cols,
      H: row, L: row + cols - 1, J: f.focus + cols * 3, K: f.focus - cols * 3, g: 0, G: n - 1,
    }[key]
    return setFocus(s, f, k, to)
  }
  if (k === 'list' || k === 'info' || k === 'thread') {
    const to = { j: f.focus + 1, k: f.focus - 1, J: f.focus + 5, K: f.focus - 5, g: 0, G: n - 1 }[key]
    if (to !== undefined) setFocus(s, f, k, to)
    return
  }
  if (k === 'blocks') {
    const to = {
      j: f.focus + 1, k: f.focus - 1, g: 0, G: n - 1,
      J: () => nextHeading(s, f, 1), K: () => nextHeading(s, f, -1),
    }[key]
    if (to !== undefined) setFocus(s, f, k, typeof to === 'function' ? to() : to)
    return
  }
  if (k === 'chars') {
    const text = s.store[f.id].text
    const c = f.cursor
    const to = {
      h: c - 1, l: c + 1, j: c + 60, k: c - 60, g: 0, G: text.length,
      H: wordStep(text, c, -1), L: wordStep(text, c, 1),
      J: sentenceStep(text, c, 1), K: sentenceStep(text, c, -1),
    }[key]
    f.cursor = Math.max(0, Math.min(text.length, to))
    return
  }
  if (k === 'photo') {
    // There is no cursor on a photo. Moving goes to the next one, and only a
    // region in progress gives hjkl something to resize.
    if (!f.extending) {
      if ('hHlL'.includes(key)) return stepSibling(s, f, 'lL'.includes(key) ? 1 : -1)
      if ('jkJK'.includes(key)) return
    }
    const big = key === 'J' || key === 'K' ? 3 : 1
    const dx = { h: -1, l: 1 }[key] ?? 0
    const dy = { k: -1, j: 1, K: -1, J: 1 }[key] ?? 0
    if (key === 'g') f.cell = { x: 0, y: 0 }
    else if (key === 'G') f.cell = { x: CELLS.x - 1, y: CELLS.y - 1 }
    else {
      f.cell = {
        x: Math.max(0, Math.min(CELLS.x - 1, f.cell.x + dx)),
        y: Math.max(0, Math.min(CELLS.y - 1, f.cell.y + dy * big)),
      }
    }
    return
  }
  if (k === 'player') {
    const o = s.store[f.id]
    const t = playerOf(s, f).t
    if (key === 'h') seek(s, f, t - 5)
    if (key === 'l') seek(s, f, t + 5)
    if (key === 'g') seek(s, f, 0)
    if (key === 'G') seek(s, f, o.dur)
    if (key === 'j') s.player.volume = Math.max(0, s.player.volume - 5)
    if (key === 'k') s.player.volume = Math.min(100, s.player.volume + 5)
    if (key === 'J') s.player.volume = Math.max(0, s.player.volume - 20)
    if (key === 'K') s.player.volume = Math.min(100, s.player.volume + 20)
    if (key === 'H' || key === 'L') {
      const dir = key === 'L' ? 1 : -1
      if (o.type === 'movie') {
        const starts = o.chapters.map((c) => c[1])
        const to = dir > 0 ? starts.find((x) => x > t) : starts.findLast((x) => x < t - 3)
        seek(s, f, to ?? (dir > 0 ? o.dur : 0))
      } else if (dir < 0 && t > 3) seek(s, f, 0)
      else stepSibling(s, f, dir)
    }
  }
}

function stepSibling(s, f, dir) {
  const stack = s.panes[s.active].stack
  const parent = stack.at(-2)
  if (!parent || !isList(kind(s, parent))) return
  const list = items(s, parent).filter((id) => s.store[id].type === s.store[f.id].type)
  const i = list.indexOf(f.id) + dir
  if (i < 0 || i >= list.length) return
  f.id = list[i]
  clearRange(f)
  parent.focus = items(s, parent).indexOf(f.id)
  if (s.store[f.id].type === 'track') play(s, f.id, stack)
}

// ---------- Going in and out ----------

function play(s, id, stack, from) {
  if (s.player.id !== id) {
    const parent = stack.at(-2)
    const queue = parent && isList(kind(s, parent)) ? items(s, parent).filter((x) => s.store[x].type === s.store[id].type) : [id]
    s.player = { ...s.player, id, t: 0, queue }
  }
  if (from !== undefined) s.player.t = from
  s.player.playing = true
}

function open(s, id, opts = {}) {
  const stack = s.panes[s.active].stack
  const o = s.store[id]
  if (o.type === 'message') {
    const t = Object.values(s.store).find((x) => x.type === 'thread' && x.items.includes(id))
    if (!t) { s.msg = 'That message isn’t in a thread any more.'; return }
    stack.push(newFrame(t.id, { focus: t.items.indexOf(id) }))
    return
  }
  if (o.type === 'thread') {
    stack.push(newFrame(id, { focus: Math.max(0, o.items.length - 1) }))
    if (o.unread) s.out.push({ do: 'read', id })
    return
  }
  stack.push(newFrame(id))
  if (o.type === 'track' || o.type === 'movie') play(s, id, stack, opts.from)
}

function activate(s) {
  const f = frame(s)
  const k = kind(s, f)
  if (k === 'player') {
    if (s.player.id !== f.id) play(s, f.id, s.panes[s.active].stack)
    else s.player.playing = !s.player.playing
    return
  }
  if (k === 'calendar') {
    const hit = underCursor(s, f)
    if (hit) open(s, hit.id)
    return
  }
  if (k === 'thread') return compose(s, f)
  if (!isList(k)) return
  const id = items(s, f)[f.focus]
  if (!id) return
  const o = s.store[id]
  if (o.type !== 'block') return open(s, id)
  if (o.ref) return open(s, o.ref, { from: o.clip?.from })
  if (o.style === 'todo') {
    dirty(s, 'tick')
    o.done = !o.done
    return
  }
  open(s, id)
}

// Apps open from anywhere. Esc comes back along the same path as ever.
function launch(s, key) {
  const id = s.store.home.items.find((x) => s.store[x].launch === key)
  if (!id) return
  const stack = s.panes[s.active].stack
  const at = stack.findIndex((x) => x.id === id && !x.view)
  if (at >= 0) stack.length = at + 1
  else open(s, id)
}

// What an action or a request is about.
export function selected(s) {
  const f = frame(s)
  const k = kind(s, f)
  return picks(k) ? targets(s, f) : [f.id]
}

// Esc undoes one step, whatever the last step was.
function back(s) {
  const f = frame(s)
  if (f.filter) { f.filter = ''; f.focus = 0; return }
  if (f.anchor !== null || f.marks.length) { clearRange(f); f.marks = []; f.base = []; return }
  const stack = s.panes[s.active].stack
  if (stack.length === 1) return
  if (s.store[f.id]?.type === 'movie' && s.player.id === f.id) s.player.playing = false
  stack.pop()
}

function select(s, key) {
  const f = frame(s)
  const k = kind(s, f)
  if (k === 'calendar' && key === 'a') {
    const week = weekStart(f.at)
    f.marks = [...new Set(occurrences(calendarEvents(s, f), week, addDays(week, 7)).map((o) => o.id))]
    clearRange(f)
    return
  }
  if (k === 'calendar' && key === 's') {
    const id = underCursor(s, f)?.id
    if (!id) { s.msg = 'No event here. S selects time.'; return }
    f.marks = f.marks.includes(id) ? f.marks.filter((x) => x !== id) : [...f.marks, id]
    return
  }
  if (key === 'a') {
    if (isList(k)) { f.marks = [...items(s, f)]; clearRange(f); return }
    if (k === 'chars') { f.anchor = 0; f.head = Math.max(0, s.store[f.id].text.length - 1); f.extending = false; return }
    if (k === 'photo') { f.anchor = { x: 0, y: 0 }; f.head = { x: CELLS.x - 1, y: CELLS.y - 1 }; f.extending = false; return }
    if (k === 'player' && s.player.id === f.id) { f.anchor = 0; f.head = s.store[f.id].dur; f.extending = false }
    return
  }
  if (key === 's' && isList(k)) {
    const id = items(s, f)[f.focus]
    if (!id) return
    f.marks = f.marks.includes(id) ? f.marks.filter((x) => x !== id) : [...f.marks, id]
    return
  }
  if (f.extending) {
    f.head = pos(s, f, k)
    f.extending = false
    return
  }
  f.anchor = pos(s, f, k)
  f.head = f.anchor
  f.extending = true
  f.base = [...f.marks]
  if (isList(k)) extendList(s, f)
}

// ---------- Tiles ----------

function rects(t, r = { x: 0, y: 0, w: 1.6, h: 1 }, out = {}) {
  if (t.pane) { out[t.pane] = r; return out }
  const [a, b] = t.dir === 'row'
    ? [{ ...r, w: r.w / 2 }, { ...r, x: r.x + r.w / 2, w: r.w / 2 }]
    : [{ ...r, h: r.h / 2 }, { ...r, y: r.y + r.h / 2, h: r.h / 2 }]
  rects(t.a, a, out)
  rects(t.b, b, out)
  return out
}

function replaceLeaf(t, pane, node) {
  if (t.pane) return t.pane === pane ? node : t
  return { ...t, a: replaceLeaf(t.a, pane, node), b: replaceLeaf(t.b, pane, node) }
}

function removeLeaf(t, pane) {
  if (t.pane) return t.pane === pane ? null : t
  const a = removeLeaf(t.a, pane)
  const b = removeLeaf(t.b, pane)
  if (!a) return b
  if (!b) return a
  return { ...t, a, b }
}

function newTile(s) {
  const f = frame(s)
  const k = kind(s, f)
  const raw = k === 'calendar' ? underCursor(s, f)?.id ?? f.id : isList(k) ? items(s, f)[f.focus] : f.id
  const id = raw ? (s.store[raw].ref ?? raw) : f.id
  const r = rects(s.layout)[s.active]
  const pane = `p${++s.paneSeq}`
  const dir = r.w >= r.h ? 'row' : 'col'
  s.layout = replaceLeaf(s.layout, s.active, { dir, a: { pane: s.active }, b: { pane } })
  s.panes[pane] = { stack: [newFrame('home')] }
  s.cols[pane] = 2
  s.active = pane
  if (id !== 'home') open(s, id)
  return ''
}

function closeTile(s) {
  if (Object.keys(s.panes).length === 1) return 'This is the only tile.'
  const r = rects(s.layout)[s.active]
  const c = { x: r.x + r.w / 2, y: r.y + r.h / 2 }
  const old = s.active
  s.layout = removeLeaf(s.layout, old)
  delete s.panes[old]
  delete s.cols[old]
  const rs = rects(s.layout)
  const dist = (p) => Math.hypot(rs[p].x + rs[p].w / 2 - c.x, rs[p].y + rs[p].h / 2 - c.y)
  s.active = Object.keys(s.panes).sort((a, b) => dist(a) - dist(b))[0]
  return ''
}

function focusTile(s, key) {
  const rs = rects(s.layout)
  const me = rs[s.active]
  const mid = { x: me.x + me.w / 2, y: me.y + me.h / 2 }
  const e = 1e-6
  const hit = Object.entries(rs).filter(([id, r]) => {
    if (id === s.active) return false
    if (key === 'h') return r.x + r.w <= me.x + e && r.y < mid.y && r.y + r.h > mid.y
    if (key === 'l') return r.x >= me.x + me.w - e && r.y < mid.y && r.y + r.h > mid.y
    if (key === 'k') return r.y + r.h <= me.y + e && r.x < mid.x && r.x + r.w > mid.x
    return r.y >= me.y + me.h - e && r.x < mid.x && r.x + r.w > mid.x
  })
  if (hit[0]) s.active = hit[0][0]
}

// ---------- Typing ----------

function startTyping(s, objId, field, at) {
  s.typing = { objId, field, pos: at ?? s.store[objId][field].length, dirtied: false, endsBlock: field === 'text' }
  s.mode = 'typing'
}

function edit(s) {
  const f = frame(s)
  const k = kind(s, f)
  if (k === 'chars') return startTyping(s, f.id, 'text', f.cursor)
  if (k === 'calendar') {
    const hit = underCursor(s, f)
    if (!hit) { s.msg = 'No event here. Press o to make one.'; return }
    startTyping(s, hit.id, 'title')
    if (s.store[hit.id].repeat) s.typing.note = 'Renaming every repeat.'
    return
  }
  if (k === 'thread') return compose(s, f)
  if (!isList(k) || k === 'info') { s.msg = NOTHING; return }
  const id = items(s, f)[f.focus]
  if (!id) return
  const o = s.store[id]
  if (s.store[f.id].fixed) { s.msg = 'Home can’t be changed.'; return }
  if (o.ref) { s.msg = 'This is a link. Press Space to open it, then edit it there.'; return }
  startTyping(s, id, o.type === 'block' ? 'text' : 'title')
}

function create(s) {
  const f = frame(s)
  const k = kind(s, f)
  if (k === 'chars') {
    s.panes[s.active].stack.pop()
    s.msg = ''
    return create(s)
  }
  if (k === 'calendar') {
    const r = range(s, f)
    dirty(s, 'new event')
    const id = newEvent(s, f, { start: r?.from ?? f.at, end: r?.to ?? f.at + 3600 })
    clearRange(f)
    startTyping(s, id, 'title')
    s.typing.dirtied = true
    s.typing.note = overlapNote(s, f, id).trim()
    return
  }
  if (k === 'thread') return compose(s, f)
  if (k === 'blocks') {
    const note = s.store[f.id]
    const after = items(s, f)[f.focus]
    const at = after ? note.items.indexOf(after) + 1 : note.items.length
    dirty(s, 'new block')
    const id = block(s, { style: 'p', text: '' })
    note.items.splice(at, 0, id)
    f.filter = ''
    f.focus = at
    startTyping(s, id, 'text')
    s.typing.dirtied = true
    return
  }
  const o = s.store[f.id]
  if ((k === 'list' || k === 'grid') && o.newType) {
    dirty(s, `new ${o.newType}`)
    const id = newId()
    if (o.newType === 'note') s.store[id] = { id, type: 'note', title: '', items: [] }
    else if (o.newType === 'thread') s.store[id] = { id, type: 'thread', title: '', peer: '', items: [] }
    else {
      const layout = o.title === 'Music' ? 'list' : 'grid'
      const accepts = o.title === 'Music' ? 'track' : 'photo'
      s.store[id] = { id, type: 'collection', title: '', layout, items: [], accepts }
    }
    const after = items(s, f)[f.focus]
    const at = after ? o.items.indexOf(after) + 1 : o.items.length
    o.items.splice(at, 0, id)
    f.filter = ''
    f.focus = at
    startTyping(s, id, 'title')
    s.typing.dirtied = true
    return
  }
  s.msg = 'Nothing new can be made here.'
}

function typeKey(s, k) {
  const t = s.typing
  const o = s.store[t.objId]
  const text = o[t.field]
  const change = (next, p) => {
    if (!t.dirtied) { dirty(s, 'edit'); t.dirtied = true }
    o[t.field] = next
    t.pos = p
  }
  if (k.key === 'Escape') {
    const f = frame(s)
    if (f.id === t.objId) f.cursor = Math.min(t.pos, text.length)
    s.mode = 'normal'
    s.typing = null
    dropEmptyDraft(s, t.objId)
  } else if (k.key === 'Enter') {
    if (t.field === 'peer' && text.trim()) {
      const peer = `~${text.trim().toLowerCase().replace(/^~/, '')}`
      if (!validPeer(peer)) { s.msg = 'That isn’t a ship name. It looks like ~sampel-palnet.'; return true }
      o.peer = peer
    }
    s.mode = 'normal'
    s.typing = null
    dropEmptyDraft(s, t.objId)
    // A thread needs someone to send to, so its name leads straight to that.
    if (o.type === 'thread' && t.field === 'title' && !o.peer) {
      startTyping(s, o.id, 'peer')
      s.typing.dirtied = true
      s.typing.note = 'Who is this with? Type their ship, like ~sampel-palnet. Esc to leave it for later.'
      return true
    }
    const f = frame(s)
    if (t.endsBlock) {
      if (f.id === t.objId) s.panes[s.active].stack.pop()
      create(s)
    }
  } else if (k.key === 'Backspace') {
    if (t.pos > 0) change(text.slice(0, t.pos - 1) + text.slice(t.pos), t.pos - 1)
  } else if (k.key === 'ArrowLeft') t.pos = Math.max(0, t.pos - 1)
  else if (k.key === 'ArrowRight') t.pos = Math.min(text.length, t.pos + 1)
  else if (k.key.length === 1 && !k.ctrl && !k.alt) {
    change(text.slice(0, t.pos) + k.key + text.slice(t.pos), t.pos + 1)
  } else return false
  return true
}

// ---------- Bar, find and menu input ----------

function runCommand(s, c) {
  s.mode = 'normal'
  s.menuPath = []
  if (c.open) {
    open(s, c.open)
    return
  }
  runVerb(s, c.run)
}

function barKey(s, k) {
  const list = completions(s)
  if (k.key === 'Escape') {
    s.mode = 'normal'
  } else if (k.key === 'Tab') {
    if (list.length) s.bar.i = (s.bar.i + (k.shift ? list.length - 1 : 1)) % list.length
  } else if (k.key === 'Enter') {
    const text = s.bar.text.trim()
    const c = list[s.bar.i]
    if (c) return runCommand(s, c)
    s.mode = 'normal'
    if (text) {
      s.out.push({ do: 'ask', text, selection: selected(s), about: targetLabel(s) })
      s.working = text
      s.msg = `Asking the agent: ${text}`
    }
  } else if (k.key === 'Backspace') {
    s.bar.text = s.bar.text.slice(0, -1)
    s.bar.i = 0
  } else if (k.key.length === 1) {
    s.bar.text += k.key
    s.bar.i = 0
  }
}

function findKey(s, k) {
  const f = frame(s)
  const kd = kind(s, f)
  if (k.key === 'Escape') {
    s.mode = 'normal'
    f.filter = ''
    f.focus = 0
    return
  }
  if (k.key === 'Enter') { s.mode = 'normal'; return }
  if (k.key === 'Backspace') f.filter = f.filter.slice(0, -1)
  else if (k.key.length === 1) f.filter += k.key
  f.focus = 0
  if (kd === 'chars' && f.filter) {
    const at = s.store[f.id].text.toLowerCase().indexOf(f.filter.toLowerCase())
    if (at >= 0) f.cursor = at
  }
  if (kd === 'calendar' && f.filter && !items(s, f).includes(underCursor(s, f)?.id)) hop(s, f, 1)
}

function menuKey(s, k) {
  if (k.key === 'Escape') {
    if (s.menuPath.length > 1) s.menuPath = s.menuPath.slice(0, -1)
    else { s.menuPath = []; s.mode = 'normal' }
    return
  }
  const e = menuEntries(s).find((x) => x.key === k.key)
  if (!e) return
  if (e.children) { s.menuPath = [...s.menuPath, e.key]; return }
  if (e.off) { s.msg = 'Mark a region first: press S, then move.'; return }
  runCommand(s, e)
}

// ---------- Key dispatch ----------

function normalKey(s, k) {
  if (k.alt) {
    if ('hjkl'.includes(k.key)) { focusTile(s, k.key); return true }
    if (k.key === 'q') { s.msg = closeTile(s); return true }
    return false
  }
  if (k.ctrl) return false
  switch (k.key) {
    case 'h': case 'j': case 'k': case 'l':
    case 'H': case 'J': case 'K': case 'L':
    case 'g': case 'G':
      move(s, k.key)
      return true
    case ' ':
      if (k.shift) s.msg = newTile(s)
      else activate(s)
      return true
    case 'Escape': back(s); return true
    case 's': case 'S': case 'a': select(s, k.key); return true
    case 'i': edit(s); return true
    case 'o': create(s); return true
    case 'x': runVerb(s, 'cut'); return true
    case 'c': runVerb(s, 'copy'); return true
    case 'v': runVerb(s, 'paste'); return true
    case 'z': runVerb(s, 'undo'); return true
    case 'Z': runVerb(s, 'redo'); return true
    case '.':
      if (s.last) runVerb(s, s.last)
      else s.msg = 'No verb to repeat yet.'
      return true
    case 'f':
      s.mode = 'menu'
      s.menuPath = []
      return true
    case 't':
      if (rootMenu(s).some((e) => e.key === 't')) {
        s.mode = 'menu'
        s.menuPath = ['t']
      } else s.msg = 'No tools work on this. D lists them all.'
      return true
    case 'n': case 'p': case 'm': case 'w': case 'e': case 'd': case 'D': case 'C': case 'M':
      launch(s, k.key)
      return true
    case ';':
      s.mode = 'bar'
      s.bar = { text: '', i: 0 }
      return true
    case '/':
      if (['photo', 'player'].includes(kind(s, frame(s)))) { s.msg = 'Nothing to find in here.'; return true }
      s.mode = 'find'
      frame(s).filter = ''
      return true
    default: {
      // App actions are one key. The menu only lists them.
      const e = rootMenu(s).find((x) => x.key === k.key)
      if (!e) return false
      if (e.off) s.msg = 'Mark a region first: press S, then move.'
      else runCommand(s, e)
      return true
    }
  }
}

function settle(s) {
  for (const pane of Object.values(s.panes)) {
    pane.stack = pane.stack.filter((f, i) => i === 0 || s.store[f.id])
    for (const f of pane.stack) {
      const k = kind(s, f)
      const n = items(s, f).length
      f.focus = Math.max(0, Math.min(n - 1, f.focus))
      if (picks(k)) f.marks = f.marks.filter((id) => s.store[id])
      if (k === 'chars') f.cursor = Math.min(f.cursor, s.store[f.id].text.length)
      if (k === 'calendar') {
        f.at ??= slotOf(s.now)
        if (f.sel && !s.store[f.id].items.includes(f.sel)) f.sel = null
      }
    }
  }
  if (!s.panes[s.active]) s.active = Object.keys(s.panes)[0]
}

// The store holds thousands of objects and every key copies it, so this stays
// shallow: nothing replaces the inside of an object except its item list.
function copyStore(store) {
  const out = {}
  for (const id in store) {
    const o = store[id]
    out[id] = o.items ? { ...o, items: [...o.items] } : { ...o }
  }
  return out
}

// key: { key, shift, alt, ctrl }
export function press(state, k) {
  const { undo: u, redo: r, store, ...rest } = state
  const s = { ...structuredClone(rest), store: copyStore(store), undo: u, redo: r, msg: '', now: k.now ?? state.now, _dirty: null, handled: true }
  const media = { MediaPlayPause: 1, MediaTrackNext: 1, MediaTrackPrevious: 1 }
  if (media[k.key]) {
    if (k.key === 'MediaPlayPause' && s.player.id) s.player.playing = !s.player.playing
    if (k.key !== 'MediaPlayPause' && s.player.id) {
      const i = s.player.queue.indexOf(s.player.id) + (k.key === 'MediaTrackNext' ? 1 : -1)
      if (s.player.queue[i]) { s.player.id = s.player.queue[i]; s.player.t = 0 }
    }
  } else if (s.mode === 'typing') s.handled = typeKey(s, k)
  else if (s.mode === 'bar') barKey(s, k)
  else if (s.mode === 'find') findKey(s, k)
  else if (s.mode === 'menu') menuKey(s, k)
  else s.handled = normalKey(s, k)
  if (s._dirty) {
    s.undo = [...s.undo, { store: state.store, label: s._dirty }].slice(-200)
    s.redo = []
  }
  delete s._dirty
  settle(s)
  return s
}

// Changes from the ship: other people, other devices, the agent.
export function applyRemote(state, objects, gone = []) {
  if (!objects.length && !gone.length) return state
  const store = { ...state.store }
  for (const o of objects) store[o.id] = o
  for (const id of gone) delete store[id]
  const s = { ...state, store, panes: structuredClone(state.panes) }
  settle(s)
  return s
}

export const say = (state, msg, extra = {}) => ({ ...state, msg, ...extra })

// A send that never reached the ship: the draft is yours again.
export function unsend(state, id) {
  const o = state.store[id]
  if (!o?.$sending) return state
  const { $sending, ...rest } = o
  return { ...state, store: { ...state.store, [id]: rest } }
}

// Open something in a tile of its own, to watch the agent work on it.
export function watch(state, id) {
  if (!state.store[id]) return state
  const s = { ...state, panes: structuredClone(state.panes), layout: structuredClone(state.layout) }
  if (!s.panes.agent) {
    s.panes.agent = { stack: [newFrame('home')] }
    s.cols.agent = 3
    s.layout = { dir: 'row', a: s.layout, b: { pane: 'agent' } }
  }
  const stack = s.panes.agent.stack
  if (stack.at(-1).id !== id) stack.push(newFrame(id))
  if (stack.length > 6) stack.splice(1, 1)
  return s
}

export function tick(state, dt) {
  const p = state.player
  if (!p.id || !p.playing) return state
  const o = state.store[p.id]
  if (o.src) return state
  let t = p.t + dt
  if (t < o.dur) return { ...state, player: { ...p, t } }
  const next = p.queue[p.queue.indexOf(p.id) + 1]
  if (o.type === 'track' && next) return { ...state, player: { ...p, id: next, t: 0 } }
  return { ...state, player: { ...p, t: o.dur, playing: false } }
}

// A real video keeps its own clock, and reports it here.
export function syncPlayer(state, id, t, ended) {
  const p = state.player
  if (p.id !== id) return state
  return { ...state, player: { ...p, t, playing: ended ? false : p.playing } }
}

export const point = (state, pane, i) => {
  const s = structuredClone({ ...state, store: null, undo: null, redo: null })
  s.store = state.store
  s.undo = state.undo
  s.redo = state.redo
  s.active = pane
  frame(s).focus = i
  s.msg = ''
  return s
}

export const setCols = (state, pane, cols) =>
  (state.cols[pane] === cols ? state : { ...state, cols: { ...state.cols, [pane]: cols } })
