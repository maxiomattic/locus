import test from 'node:test'
import assert from 'node:assert/strict'
import { init, press, frame, items, titleOf, kind, targetLabel, range, viewActions, applyRemote } from '../src/engine.js'
import { seedStore } from '../src/data.js'
import { lanes, occurrences } from '../src/calendar.js'

const KEY = { sp: ' ', esc: 'Escape', ent: 'Enter' }
function type(s, seq) {
  for (const t of seq.split(' ')) {
    if (t === '') continue
    if (KEY[t]) s = press(s, { key: KEY[t] })
    else for (const c of t) s = press(s, { key: c, shift: c !== c.toLowerCase() })
  }
  return s
}
const find = (s, title) => Object.values(s.store).find((o) => o.title === title)
const when = (d, h, m = 0) => new Date(2026, 10, d, h, m) / 1000
const NOW = when(11, 9) // Wednesday 11 November 2026, 9:00
function week(events = []) {
  const store = seedStore()
  store.calendar.items = []
  for (const e of events) {
    store[e.id] = { type: 'event', items: [], ...e }
    store.calendar.items.push(e.id)
  }
  return type(init(store, NOW), 'C')
}
const ev = (id, title, start, end, extra = {}) => ({ id, title, start, end, ...extra })

test('a selected time becomes an event, and z takes it back', () => {
  let s = week()
  assert.equal(kind(s, frame(s)), 'calendar')
  s = type(s, 'S j j j S')
  assert.deepEqual(range(s, frame(s)), { from: when(11, 9), to: when(11, 11) })
  s = type(s, 'o Standup esc')
  const e = find(s, 'Standup')
  assert.equal(e.start, when(11, 9))
  assert.equal(e.end, when(11, 11))
  assert.equal(range(s, frame(s)), null)
  s = type(s, 'z')
  assert.equal(find(s, 'Standup'), undefined)
})

test('a slot makes a one hour event, and Space opens it like a note', () => {
  let s = type(week(), 'o Lunch esc')
  assert.equal(find(s, 'Lunch').end - find(s, 'Lunch').start, 3600)
  s = type(s, 'sp o hello esc')
  assert.equal(kind(s, frame(s)), 'blocks')
  assert.equal(titleOf(s, find(s, 'Lunch').items[0]), 'hello')
  s = type(s, 'esc')
  assert.equal(kind(s, frame(s)), 'calendar')
})

test('cut and paste moves an event to the cursor, keeping its length', () => {
  let s = week([ev('a', 'Dentist', when(11, 9), when(11, 10, 30))])
  s = type(s, 'x')
  assert.deepEqual(s.store.calendar.items, [])
  s = type(s, 'l j j v')
  assert.equal(s.store.a.start, when(12, 10))
  assert.equal(s.store.a.end, when(12, 11, 30))
  s = type(s, 'z')
  assert.deepEqual(s.store.calendar.items, [])
  assert.equal(s.store.a.start, when(11, 9))
})

test('several events paste with their distances kept', () => {
  let s = week([ev('a', 'One', when(11, 9), when(11, 10)), ev('b', 'Two', when(12, 11), when(12, 12))])
  s = type(s, 's J s x')
  assert.deepEqual(s.held.ids, ['a', 'b'])
  s = type(s, 'L v')
  assert.equal(s.store.a.start, when(19, 11))
  assert.equal(s.store.b.start, when(20, 13))
})

test('copy makes a new event with its own blocks', () => {
  let s = week([ev('a', 'One', when(11, 9), when(11, 10), { items: ['b1'] })])
  s.store.b1 = { id: 'b1', type: 'block', style: 'p', text: 'x' }
  s = type(s, 'c l v')
  const copy = Object.values(s.store).find((o) => o.type === 'event' && o.id !== 'a' && o.title === 'One')
  assert.equal(copy.title, 'One')
  assert.equal(copy.start, when(12, 9))
  assert.notEqual(copy.items[0], 'b1')
  assert.equal(s.store[copy.items[0]].text, 'x')
})

test('text and objects pasted on a time become events; a clip refuses', () => {
  let s = week()
  s.held = { kind: 'text', text: 'Pay rent', label: 'Pay rent' }
  s = type(s, 'v')
  assert.equal(find(s, 'Pay rent').type, 'event')
  const lauren = find(s, 'Lauren Okafor')
  s.held = { kind: 'items', ids: [lauren.id], cut: false, label: 'Lauren Okafor' }
  s = type(s, 'l v')
  const made = Object.values(s.store).find((o) => o.type === 'event' && o.title === 'Lauren Okafor')
  assert.equal(s.store[made.items[0]].ref, lauren.id)
  s.held = { kind: 'clip', id: 'x', from: 0, to: 1, label: 'a clip' }
  s = type(s, 'l v')
  assert.match(s.msg, /can’t hold/)
})

test('an event pasted in a note is a reference to it', () => {
  let s = week([ev('a', 'One', when(11, 9), when(11, 10))])
  s = type(s, 'c n sp v')
  assert.ok(items(s, frame(s)).some((id) => s.store[id].ref === 'a'))
})

test('repeats expand, and moving one moves every repeat', () => {
  let s = week([ev('a', 'Review', when(4, 9), when(4, 10), { repeat: 'weekly' })])
  assert.equal(targetLabel(s), 'Review')
  s = type(s, 'L')
  assert.equal(targetLabel(s), 'Review')
  s = type(s, 'x')
  assert.match(s.msg, /every repeat/)
  s = type(s, 'H l v')
  assert.equal(targetLabel(s), 'Review')
  assert.equal(s.store.a.repeat, 'weekly')
  assert.equal(occurrences([s.store.a], when(9, 0), when(16, 0))[0].start, when(12, 9))
})

test('a daily repeat keeps its hour across a clock change', () => {
  const tz = process.env.TZ
  process.env.TZ = 'America/New_York'
  try {
    const o = { id: 'a', start: new Date(2026, 9, 30, 9) / 1000, end: new Date(2026, 9, 30, 10) / 1000, repeat: 'daily' }
    const list = occurrences([o], new Date(2026, 9, 30) / 1000, new Date(2026, 10, 4) / 1000)
    assert.equal(list.length, 5)
    for (const e of list) assert.equal(new Date(e.start * 1000).getHours(), 9)
  } finally { if (tz === undefined) delete process.env.TZ; else process.env.TZ = tz }
})

test('y cycles daily, weekly, none', () => {
  let s = week([ev('a', 'One', when(11, 9), when(11, 10))])
  s = type(s, 'y')
  assert.equal(s.store.a.repeat, 'daily')
  s = type(s, 'y')
  assert.equal(s.store.a.repeat, 'weekly')
  s = type(s, 'y')
  assert.equal(s.store.a.repeat, null)
})

test('< and > change length, stop at one slot, and . repeats', () => {
  let s = week([ev('a', 'One', when(11, 9), when(11, 10))])
  s = type(s, '> .')
  assert.equal(s.store.a.end, when(11, 11))
  s = type(s, '< < < < <')
  assert.equal(s.store.a.end, when(11, 9, 30))
})

test('stepping days and pasting keeps the wall clock across daylight saving', () => {
  const tz = process.env.TZ
  process.env.TZ = 'America/New_York'
  try {
    const store = seedStore()
    store.calendar.items = []
    let s = type(init(store, new Date(2026, 9, 31, 9) / 1000), 'C') // clocks go back on 1 November
    s = type(s, 'l l')
    assert.equal(new Date(frame(s).at * 1000).getHours(), 9)
    s = type(s, 'o Monday esc x h v')
    const d = new Date(find(s, 'Monday').start * 1000)
    assert.equal(d.getHours(), 9)
    assert.equal(d.getDate(), 1)
  } finally { if (tz === undefined) delete process.env.TZ; else process.env.TZ = tz }
})

test('j and k stay inside the day', () => {
  let s = week()
  for (let i = 0; i < 60; i++) s = type(s, 'j')
  assert.equal(frame(s).at, when(11, 23, 30))
  for (let i = 0; i < 60; i++) s = type(s, 'k')
  assert.equal(frame(s).at, when(11, 0))
})

test('an overlap is named when an event lands on one', () => {
  let s = week([ev('a', 'One', when(11, 9), when(11, 10))])
  s = type(s, 'c j v')
  assert.match(s.msg, /Overlaps One/)
  s = week([ev('a', 'One', when(11, 9), when(11, 10))])
  s = type(s, 'o')
  assert.equal(s.typing.note, 'Overlaps One.')
})

test('overlapping events share the width', () => {
  const out = lanes([{ top: 0, bottom: 4 }, { top: 2, bottom: 6 }, { top: 8, bottom: 9 }])
  assert.deepEqual(out.map((g) => [g.lane, g.lanes]), [[0, 2], [1, 2], [0, 1]])
})

test('J and K step through events that share a start, one at a time', () => {
  let s = week([ev('a', 'One', when(11, 10), when(11, 11)), ev('b', 'Two', when(11, 10), when(11, 11)), ev('c', 'Three', when(11, 12), when(11, 13))])
  s = type(s, 'J')
  assert.equal(targetLabel(s), 'One')
  s = type(s, 'J')
  assert.equal(targetLabel(s), 'Two')
  s = type(s, 'x')
  assert.deepEqual(s.held.ids, ['b'])
  s = type(s, 'z J J')
  assert.equal(targetLabel(s), 'Three')
  s = type(s, 'K K')
  assert.equal(targetLabel(s), 'One')
})

test('find hops only what matches', () => {
  let s = week([ev('a', 'One', when(11, 10), when(11, 11)), ev('b', 'Two', when(11, 12), when(11, 13)), ev('c', 'One more', when(11, 14), when(11, 15))])
  s = type(s, '/ one ent')
  assert.equal(targetLabel(s), 'One')
  s = type(s, 'J')
  assert.equal(targetLabel(s), 'One more')
  s = type(s, 'esc')
  assert.equal(frame(s).filter, '')
})

test('Esc clears a selection, then leaves the calendar', () => {
  let s = type(week(), 'S j esc')
  assert.equal(range(s, frame(s)), null)
  assert.equal(kind(s, frame(s)), 'calendar')
  s = type(s, 'esc')
  assert.equal(frame(s).id, 'home')
})

test('tools and the agent act on the event under the cursor', () => {
  let s = week([ev('a', 'One', when(11, 9), when(11, 10))])
  s = applyRemote(s, [{ id: 'tool:t', type: 'tool', name: 'Prep', key: 'B', appliesTo: 'event' }])
  assert.ok(viewActions(s).some(([k, l]) => k === 't' && l === 'Tools'))
  s = type(s, 't B')
  assert.deepEqual(s.out[0].selection, ['a'])
  s = press(s, { key: ';' })
  s = type(s, 'xyzzy ent')
  assert.ok(s.out.some((e) => e.do === 'ask' && e.selection[0] === 'a'))
})

test('a selects the week', () => {
  let s = week([ev('a', 'One', when(11, 9), when(11, 10)), ev('b', 'Two', when(13, 9), when(13, 10)), ev('c', 'Far', when(25, 9), when(25, 10))])
  s = type(s, 'a')
  assert.deepEqual(frame(s).marks, ['a', 'b'])
  assert.equal(targetLabel(s), '2 events')
})

test('t comes back to today, and g G reach the ends of the week', () => {
  let s = week()
  s = type(s, 'L L u')
  assert.equal(frame(s).at, NOW)
  s = type(s, 'g')
  assert.equal(frame(s).at, when(9, 9))
  s = type(s, 'G')
  assert.equal(frame(s).at, when(15, 9))
})

test('info is about the event under the cursor', () => {
  const s = type(week([ev('a', 'One', when(11, 9), when(11, 10))]), 'I')
  assert.equal(frame(s).view, 'info')
  assert.equal(frame(s).id, 'a')
})
