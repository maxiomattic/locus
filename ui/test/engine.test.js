import { diff } from '../src/sync.js'
import test from 'node:test'
import assert from 'node:assert/strict'
import { init, press, frame, items, titleOf, kind, targetLabel, completions, range, tick, viewActions, visibleSize, syncPlayer, applyRemote } from '../src/engine.js'
import { seedStore } from '../src/data.js'
import { withMedia } from '../src/media.js'

const KEY = { sp: ' ', esc: 'Escape', ent: 'Enter', tab: 'Tab', bs: 'Backspace' }
function type(s, seq) {
  for (const t of seq.split(' ')) {
    if (t === '') continue
    if (t === 'S-sp') s = press(s, { key: ' ', shift: true })
    else if (t.startsWith('A-')) s = press(s, { key: t.slice(2), alt: true })
    else if (KEY[t]) s = press(s, { key: KEY[t] })
    else for (const c of t) s = press(s, { key: c, shift: c !== c.toLowerCase() })
  }
  return s
}
const focused = (s) => titleOf(s, items(s, frame(s))[frame(s).focus])
const find = (s, title) => Object.values(s.store).find((o) => o.title === title)

test('hjkl moves spatially on a grid', () => {
  let s = init()
  s = type(s, 'l')
  assert.equal(focused(s), 'Photos')
  s = type(s, 'j')
  assert.equal(focused(s), 'Inbox')
  s = type(s, 'k h')
  assert.equal(focused(s), 'Notes')
})

test('Space goes in, Esc comes out', () => {
  let s = type(init(), 'sp')
  assert.equal(titleOf(s, frame(s).id), 'Notes')
  s = type(s, 'sp')
  assert.equal(kind(s, frame(s)), 'blocks')
  s = type(s, 'esc esc')
  assert.equal(frame(s).id, 'home')
  s = type(s, 'esc')
  assert.equal(frame(s).id, 'home')
})

test('Esc clears the selection before it leaves', () => {
  let s = type(init(), 'l sp sp s l s')
  assert.equal(frame(s).marks.length, 2)
  s = type(s, 'esc')
  assert.equal(frame(s).marks.length, 0)
  assert.equal(titleOf(s, frame(s).id), 'Portugal')
})

test('cut and paste moves photos between albums, and undo restores', () => {
  let s = type(init(), 'l sp sp s l s')
  assert.equal(targetLabel(s), '2 photos')
  s = type(s, 'x')
  assert.equal(find(s, 'Portugal').items.length, 4)
  s = type(s, 'esc l sp v')
  assert.equal(find(s, 'Near home').items.length, 6)
  s = type(s, 'z z')
  assert.equal(find(s, 'Near home').items.length, 4)
  assert.equal(find(s, 'Portugal').items.length, 6)
  s = type(s, 'Z')
  assert.equal(find(s, 'Portugal').items.length, 4)
})

test('range selection extends as you move', () => {
  const s = type(init(), 'l sp sp S l l')
  assert.equal(frame(s).marks.length, 3)
  assert.equal(type(s, 'S l').marks, undefined)
  assert.equal(frame(type(s, 'S l')).marks.length, 3)
})

test('a pasted photo is a reference, so a rotate shows up in the note', () => {
  let s = type(init(), 'l sp sp c esc esc esc h sp sp v')
  const note = find(s, 'Trip to Lisbon')
  const b = s.store[note.items[1]]
  assert.equal(s.store[b.ref].title, 'Lisbon tram')
  s = type(s, 'esc esc esc l sp sp r')
  assert.equal(find(s, 'Lisbon tram').rot, 90)
})

test('following a reference, Esc returns along your path', () => {
  let s = type(init(), 'sp sp j j')
  assert.equal(focused(s), 'Lauren Okafor')
  s = type(s, 'sp')
  assert.equal(titleOf(s, frame(s).id), 'Lauren Okafor')
  s = type(s, 'esc')
  assert.equal(titleOf(s, frame(s).id), 'Trip to Lisbon')
})

test('typing captures keys, and undo takes it back in one step', () => {
  let s = type(init(), 'sp sp j i')
  assert.equal(s.mode, 'typing')
  s = type(s, 'hjkl esc')
  assert.equal(s.mode, 'normal')
  assert.ok(focused(s).endsWith('hjkl'))
  s = type(s, 'z')
  assert.ok(!focused(s).endsWith('hjkl'))
})

test('o creates a sibling and starts typing', () => {
  let s = type(init(), 'sp o')
  assert.equal(s.mode, 'typing')
  s = type(s, 'Groceries ent')
  assert.equal(focused(s), 'Groceries')
  assert.equal(find(s, 'Notes').items.length, 4)
})

test('Enter while typing a block makes the next block', () => {
  let s = type(init(), 'sp sp o')
  s = type(s, 'one ent two esc')
  const items_ = find(s, 'Trip to Lisbon').items.map((id) => s.store[id].text)
  assert.ok(items_.includes('one') && items_.includes('two'))
})

test('Space ticks a to-do', () => {
  let s = type(init(), 'sp sp j j j')
  assert.equal(focused(s), 'Book the tram tour')
  s = type(s, 'sp')
  const b = s.store[items(s, frame(s))[frame(s).focus]]
  assert.equal(b.done, true)
  assert.equal(kind(s, frame(s)), 'blocks')
})

test('characters: enter a block, select a range, cut, paste', () => {
  let s = type(init(), 'sp sp j sp')
  assert.equal(kind(s, frame(s)), 'chars')
  s = type(s, 'S L')
  const r = range(s, frame(s))
  assert.equal(r.from, 0)
  s = type(s, 'x')
  assert.equal(s.held.kind, 'text')
  assert.equal(frame(s).cursor, 0)
  s = type(s, 'v')
  assert.ok(s.store[frame(s).id].text.startsWith('Flights land'))
})

test('player: Space pauses, hl seek, jk volume, HL change track', () => {
  let s = type(init(), 'l l sp sp sp')
  assert.equal(kind(s, frame(s)), 'player')
  assert.equal(s.player.playing, true)
  assert.equal(titleOf(s, s.player.id), 'Harbor light')
  s = type(s, 'sp')
  assert.equal(s.player.playing, false)
  s = type(s, 'l l')
  assert.equal(s.player.t, 10)
  s = type(s, 'h')
  assert.equal(s.player.t, 5)
  const v = s.player.volume
  s = type(s, 'k k j')
  assert.equal(s.player.volume, v + 5)
  s = type(s, 'L')
  assert.equal(titleOf(s, s.player.id), 'Slow ferry')
  assert.equal(titleOf(s, frame(s).id), 'Slow ferry')
})

test('H and L jump chapters in a movie, and leaving pauses it', () => {
  let s = type(init(), 'l l l sp sp')
  assert.equal(s.player.playing, true)
  s = type(s, 'L')
  assert.equal(s.player.t, 1320)
  s = type(s, 'esc')
  assert.equal(s.player.playing, false)
})

test('a marked clip pastes into a note and opens at its start', () => {
  let s = type(init(), 'l l l sp sp sp L S L c')
  assert.equal(s.held.kind, 'clip')
  s = type(s, 'esc esc esc h h h sp sp v')
  const f = frame(s)
  const b = s.store[items(s, f)[f.focus]]
  assert.equal(b.clip.from, 1320)
  s = type(s, 'sp')
  assert.equal(s.player.t, 1320)
})

test('local verbs do nothing on a leaf with no selection', () => {
  let s = type(init(), 'l sp sp sp')
  assert.equal(kind(s, frame(s)), 'photo')
  const before = find(s, 'Portugal').items.length
  s = type(s, 'x')
  assert.equal(find(s, 'Portugal').items.length, before)
  assert.match(s.msg, /Nothing selected/)
})

test('rotate works on the active photo, and on a selection in the grid', () => {
  let s = type(init(), 'l sp sp sp r')
  assert.equal(find(s, 'Lisbon tram').rot, 90)
  s = type(s, 'esc s l s r')
  assert.equal(find(s, 'Lisbon tram').rot, 180)
  assert.equal(find(s, 'Alfama rooftops').rot, 90)
})

test('r repeats the last verb on a new target', () => {
  let s = type(init(), 'l sp sp r l . l .')
  assert.equal(find(s, 'Alfama rooftops').rot, 90)
  assert.equal(find(s, 'Belém at dusk').rot, 90)
})

test('crop needs a region, and a region crops without leaving the photo', () => {
  let s = type(init(), 'l sp sp sp C')
  assert.equal(find(s, 'Lisbon tram').crop, null)
  s = type(s, 'S l l j C')
  const c = find(s, 'Lisbon tram').crop
  assert.ok(c && c.w > 0 && c.h > 0)
  assert.equal(kind(s, frame(s)), 'photo')
})

test('crop maps correctly after a rotate', () => {
  let s = type(init(), 'l sp sp sp f r g S l j C')
  const c = find(s, 'Lisbon tram').crop
  // Top left of a clockwise-rotated picture is its bottom left corner.
  assert.equal(c.x, 0)
  assert.equal(c.y + c.h, 800)
})

test('app actions take one key, and the menu uses the same letters', () => {
  let s = type(init(), 'sp sp j b')
  assert.equal(s.mode, 'normal')
  assert.equal(s.store[items(s, frame(s))[1]].style, 'h')
  s = type(s, 'f t')
  assert.equal(s.mode, 'normal')
  assert.equal(s.store[items(s, frame(s))[1]].style, 'todo')
  s = type(s, 'f esc')
  assert.equal(s.mode, 'normal')
})

test('info shows what links to an object', () => {
  let s = type(init(), 'sp sp j j I')
  assert.equal(kind(s, frame(s)), 'info')
  assert.ok(items(s, frame(s)).map((id) => titleOf(s, id)).includes('Trip to Lisbon'))
})

test('command bar runs commands, opens things, and falls through to the agent', () => {
  let s = type(init(), 'l sp sp ;')
  assert.equal(s.mode, 'bar')
  s = type(s, 'rot')
  assert.equal(completions(s)[0].label, 'Rotate')
  s = type(s, 'ent')
  assert.equal(find(s, 'Lisbon tram').rot, 90)
  s = type(s, '; lauren ent')
  assert.equal(titleOf(s, frame(s).id), 'Lauren Okafor')
  s = type(s, '; maketh')
  s = type(s, 'iszzzz ent')
  assert.equal(s.out[0].text, 'makethiszzzz')
  assert.equal(s.mode, 'normal')
})

test('find narrows the view, and Esc clears it', () => {
  let s = type(init(), 'l sp sp /')
  s = type(s, 'sin ent')
  assert.deepEqual(items(s, frame(s)).map((id) => titleOf(s, id)), ['Sintra hills'])
  s = type(s, 'esc')
  assert.equal(items(s, frame(s)).length, 6)
})

test('tiles: open beside, move between, close', () => {
  let s = type(init(), 'S-sp')
  assert.equal(Object.keys(s.panes).length, 2)
  assert.equal(titleOf(s, frame(s).id), 'Notes')
  const right = s.active
  s = type(s, 'A-h')
  assert.notEqual(s.active, right)
  s = type(s, 'A-l')
  assert.equal(s.active, right)
  s = type(s, 'A-q')
  assert.equal(Object.keys(s.panes).length, 1)
  s = type(s, 'A-q')
  assert.match(s.msg, /only tile/)
})

test('paste works across tiles', () => {
  let s = type(init(), 'l sp sp S-sp')
  assert.equal(titleOf(s, frame(s).id), 'Lisbon tram')
  s = type(s, 'A-h c')
  s = type(s, ';')
  s = type(s, 'trip ent')
  assert.equal(titleOf(s, frame(s).id), 'Trip to Lisbon')
  s = type(s, 'v')
  const b = s.store[items(s, frame(s))[1]]
  assert.equal(s.store[b.ref].title, 'Lisbon tram')
})

test('music moves to the next track on its own', () => {
  let s = type(init(), 'l l sp sp sp')
  s = tick(s, 1000)
  assert.equal(titleOf(s, s.player.id), 'Slow ferry')
})

test('home cannot be cut', () => {
  const s = type(init(), 'x')
  assert.equal(items(s, frame(s)).length, 8)
})

test('h and l step through photos, and hjkl only resize a region in progress', () => {
  let s = type(init(), 'l sp sp sp l')
  assert.equal(titleOf(s, frame(s).id), 'Alfama rooftops')
  s = type(s, 'h')
  assert.equal(titleOf(s, frame(s).id), 'Lisbon tram')
  s = type(s, 'S l')
  assert.equal(titleOf(s, frame(s).id), 'Lisbon tram')
  assert.equal(range(s, frame(s)).w, 2)
})

test('letters launch apps from anywhere, and Esc returns along the path', () => {
  let s = type(init(), 'p')
  assert.equal(titleOf(s, frame(s).id), 'Photos')
  s = type(s, 'sp w')
  assert.equal(titleOf(s, frame(s).id), 'Movies')
  s = type(s, 'esc')
  assert.equal(titleOf(s, frame(s).id), 'Portugal')
  s = type(s, 'p')
  assert.equal(titleOf(s, frame(s).id), 'Photos')
  s = type(s, 'n sp j i n')
  assert.equal(s.mode, 'typing')
  assert.ok(focused(s).endsWith('n'))
})

test('views list their actions with live keys', () => {
  const labels = (s) => viewActions(s).map(([k, l]) => `${k} ${l}`)
  assert.ok(labels(type(init(), 'l sp sp sp')).includes('r Rotate'))
  assert.ok(labels(type(init(), 'l l l sp sp')).includes('Space Pause'))
  assert.ok(labels(type(init(), 'l sp sp sp')).includes('C Crop'))
})

test('the real library replaces the sample media without touching the rest', () => {
  const m = {
    albums: [{ title: '2025', items: [{ id: 'a1', title: 'PXL', w: 3000, h: 4000 }] }],
    movies: { folders: [{ title: 'Series', folders: [], movies: [{ id: 'm2', title: 'Two', dur: 60, chapters: [['Start', 0]] }] }], movies: [{ id: 'm1', title: 'One', dur: 90, chapters: [['Start', 0]] }] },
  }
  let s = init(withMedia(seedStore(), m))
  assert.equal(find(s, 'Lisbon tram'), undefined)
  assert.equal(find(s, 'The Long Crossing'), undefined)
  assert.equal(find(s, 'Trip to Lisbon').items.some((id) => !s.store[id]), false)
  s = type(s, 'p sp sp')
  assert.equal(kind(s, frame(s)), 'photo')
  assert.deepEqual(visibleSize(s.store[frame(s).id]), { w: 3000, h: 4000 })
  s = type(s, 'w')
  assert.deepEqual(items(s, frame(s)).map((id) => titleOf(s, id)), ['One', 'Series'])
  s = type(s, 'sp')
  const t = syncPlayer(s, s.player.id, 12, false)
  assert.equal(tick(t, 5).player.t, 12)
})

test('the store difference becomes typed operations', () => {
  const a = { n: { id: 'n', type: 'note', title: 'x', items: ['b', 'c'], $rev: 3 }, gone: { id: 'gone', type: 'note' } }
  const b = { n: { ...a.n, title: 'y', items: ['b', 'd', 'c'] }, m: { id: 'm', type: 'note', title: 'new' } }
  const ops = diff(a, b)
  assert.deepEqual(ops.find((o) => o.do === 'splice'), { do: 'splice', id: 'n', field: 'items', del: [], add: [{ after: 'b', val: 'd' }] })
  assert.deepEqual(ops.find((o) => o.do === 'set'), { do: 'set', id: 'n', base: 3, fields: { title: 'y' } })
  assert.deepEqual(ops.find((o) => o.do === 'make'), { do: 'make', id: 'm', type: 'note', fields: { title: 'new' } })
  assert.deepEqual(ops.find((o) => o.do === 'trash'), { do: 'trash', id: 'gone' })
})

test('a reorder falls back to writing the whole list', () => {
  const a = { n: { id: 'n', type: 'note', items: ['a', 'b'], $rev: 1 } }
  const ops = diff(a, { n: { ...a.n, items: ['b', 'a'] } })
  assert.deepEqual(ops, [{ do: 'set', id: 'n', base: 1, fields: { items: ['b', 'a'] } }])
})

test('the inbox lists open proposals, and y approves through the ship', () => {
  let s = init()
  s = applyRemote(s, [{ id: 'p1', type: 'proposal', status: 'open', intent: 'Tag these', actor: 'agent:a', ops: [] }])
  s = type(s, 'd')
  assert.equal(titleOf(s, frame(s).id), 'Inbox')
  assert.equal(items(s, frame(s)).length, 1)
  s = type(s, 'sp y')
  assert.deepEqual(s.out, [{ do: 'decide', id: 'p1', ok: true }])
})

test('a learned tool appears as one key on what it applies to', () => {
  let s = init()
  s = applyRemote(s, [{ id: 'tool:t', type: 'tool', name: 'Warm up', key: 'B', appliesTo: 'photo' }])
  s = type(s, 'l sp sp sp')
  assert.ok(viewActions(s).some(([k, l]) => k === 'B' && l === 'Warm up'))
  s = type(s, 'B')
  assert.equal(s.out[0].do, 'tool')
  assert.equal(s.out[0].id, 'tool:t')
})

test('both versions of the command bar stay built, and the choice syncs', () => {
  let s = type(init(), '; floating ent')
  assert.equal(s.store.prefs.bar, 'float')
  s = type(s, '; top ent')
  assert.equal(s.store.prefs.bar, 'line')
})

test('history lists changes and u undoes one through the ship', () => {
  let s = type(init(), '; history ent')
  assert.equal(s.out[0].do, 'history')
  s = { ...s, out: [], history: [{ seq: 7, actor: 'human', intent: 'x', ops: [], at: 1 }] }
  s = type(s, 'u')
  assert.deepEqual(s.out, [{ do: 'revert', seq: 7 }])
})

test('trash lists trashed objects and restores them', () => {
  let s = applyRemote(init(), [{ id: 'old', type: 'note', title: 'Old', items: [], $trashed: true }])
  s = type(s, '; trash ent')
  assert.equal(items(s, frame(s)).length, 1)
  s = type(s, 'u')
  assert.deepEqual(s.out, [{ do: 'restore', id: 'old' }])
})

test('a clash on what you are in can be settled either way', () => {
  let s = type(init(), 'n sp')
  const id = items(s, frame(s))[0]
  s = applyRemote(s, [{ ...s.store[id], $clashes: [{ field: 'text', theirs: 'yours', actor: 'human', seq: 3 }] }])
  s = type(s, 'sp')
  assert.ok(viewActions(s).some(([k]) => k === 'O'))
  s = type(s, 'O')
  assert.deepEqual(s.out, [{ do: 'resolve', id, field: 'text', keep: true }])
})
