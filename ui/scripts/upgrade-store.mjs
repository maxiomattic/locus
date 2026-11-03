// Adds objects newer versions of the interface expect to a store seeded before them.
import { messageSetup } from '../server/messages.mjs'

const url = process.env.LOCUS_URL ?? 'http://127.0.0.1:5180'
const s = await fetch(`${url}/store/~/v1/state`).then((r) => r.json())
const has = (id) => s.objects.some((o) => o.id === id)
const ops = []
if (!has('trash')) ops.push({ do: 'make', id: 'trash', type: 'collection', fields: { title: 'Trash', layout: 'list', items: [], fixed: true, query: { $trashed: true } } })
// The Keys note, for stores seeded before the tools leader and the approval keys.
const KEYS_TEXT = [
  ['From anywhere: n Notes, p Photos, m Music, w Movies, e People, d Inbox, D Tools.',
    'From anywhere: n Notes, p Photos, m Music, w Movies, e People, C Calendar, M Messages, d Inbox, D Tools.'],
  ['; then a request asks the agent. What it wants to change waits in the Inbox: y approves, r rejects. Ask for the same thing twice and it offers a tool, which then has a key of its own and runs without the agent.',
    '; then a request asks the agent. What it wants to change waits in the Inbox: A approves, R rejects. Ask for the same thing twice and it offers a tool, which then sits behind t and runs without the agent.'],
  ['Each view lists its own one-key actions at the top right, such as r to rotate a photo. f lists everything. ; opens the command bar, which is also where you talk to the agent.',
    'Each view lists its own one-key actions at the top right, such as r to rotate a photo. Tools you have taught the agent are behind t, then their key. f lists everything. ; opens the command bar, which is also where you talk to the agent.'],
]
for (const [was, now] of KEYS_TEXT) {
  for (const o of s.objects) {
    if (o.type === 'block' && o.fields.text === was) ops.push({ do: 'set', id: o.id, base: o.rev ?? 0, fields: { text: now } })
  }
}
if (!has('type:event')) ops.push({ do: 'make', id: 'type:event', type: 'type', fields: { name: 'event', fields: { title: 'text', start: 'num', end: 'num', repeat: 'text', items: 'refs' } } })
if (!has('calendar')) {
  const people = s.objects.find((o) => o.fields.launch === 'e')
  ops.push({ do: 'make', id: 'calendar', type: 'collection', fields: { title: 'Calendar', layout: 'calendar', items: [], accepts: 'event', launch: 'C' } })
  ops.push({ do: 'splice', id: 'home', field: 'items', del: [], add: [{ after: people?.id ?? null, val: 'calendar' }] })
}
const keysNote = s.objects.find((o) => o.type === 'note' && o.fields.title === 'Keys')
if (keysNote && !has('keys-calendar')) {
  const text = 'C opens the week. j k move half an hour, h l a day, H L a week, J K the next or previous event, u today. S selects time, then o makes an event from it. x and v move an event, c and v copy it, < and > change its length, y repeats it. Changing a repeating event changes every repeat.'
  ops.push({ do: 'make', id: 'keys-calendar-h', type: 'block', fields: { style: 'h', text: 'Calendar' } })
  ops.push({ do: 'make', id: 'keys-calendar', type: 'block', fields: { style: 'p', text } })
  ops.push({ do: 'splice', id: keysNote.id, field: 'items', del: [], add: [{ after: keysNote.fields.items.at(-1) ?? null, val: 'keys-calendar-h' }, { after: 'keys-calendar-h', val: 'keys-calendar' }] })
}
if (!has('messages')) {
  ops.push({ do: 'make', id: 'messages', type: 'collection', fields: { title: 'Messages', layout: 'list', items: [], accepts: 'thread', newType: 'thread', launch: 'M' } })
  const home = s.objects.find((o) => o.id === 'home')
  if (home) ops.push({ do: 'splice', id: 'home', field: 'items', del: [], add: [{ after: home.fields.items.at(-1) ?? null, val: 'messages' }] })
}
const keysFor = s.objects.find((o) => o.type === 'note' && o.fields.title === 'Keys')
if (keysFor && !has('keys-messages')) {
  const text = 'M opens Messages. o in a thread writes your draft, and Enter or Esc stops typing and keeps it. Enter never sends. A sends, the same key that approves a proposal. P sets who a thread is with. A sent message can’t be unsent.'
  ops.push({ do: 'make', id: 'keys-messages-h', type: 'block', fields: { style: 'h', text: 'Messages' } })
  ops.push({ do: 'make', id: 'keys-messages', type: 'block', fields: { style: 'p', text } })
  ops.push({ do: 'splice', id: keysFor.id, field: 'items', del: [], add: [{ after: has('keys-calendar') ? 'keys-calendar' : keysFor.fields.items.at(-1) ?? null, val: 'keys-messages-h' }, { after: 'keys-messages-h', val: 'keys-messages' }] })
}
// Stores that already took the comma leader, then the same text moved to t.
const SWAPS = [['are behind , then their key', 'are behind t, then their key'], ['sits behind , and', 'sits behind t and'], ['event, t today.', 'event, u today.']]
for (const o of s.objects) {
  if (o.type !== 'block' || typeof o.fields.text !== 'string') continue
  const text = SWAPS.reduce((t, [was, now]) => t.replace(was, now), o.fields.text)
  if (text !== o.fields.text) ops.push({ do: 'set', id: o.id, base: o.rev ?? 0, fields: { text } })
}
for (const op of messageSetup((fields) => ({ do: 'make', id: `type:${fields.name}`, type: 'type', fields }))) if (!has(op.id)) ops.push(op)
if (ops.length) console.log(await fetch(`${url}/store/~/v1/do`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ do: 'batch', intent: 'upgrade', ops }) }).then((r) => r.text()))
