// Everything is an object in one store. Views are only ways of looking at it.
import { addDays, weekStart } from './calendar.js'

const s = {}
let n = 0
const add = (o) => { const id = o.id ?? `o${++n}`; s[id] = { ...o, id }; return id }
const p = (text, style = 'p', extra = {}) => add({ type: 'block', style, text, ...extra })
const ref = (id, extra = {}) => add({ type: 'block', ref: id, ...extra })
const note = (title, items, extra = {}) => add({ type: 'note', title, items, ...extra })
const photo = (title, seed) => add({ type: 'photo', title, seed, rot: 0, crop: null })
const track = (title, dur, seed) => add({ type: 'track', title, dur, seed })
const movie = (title, dur, seed, chapters) => add({ type: 'movie', title, dur, seed, chapters })
const coll = (title, layout, items, extra = {}) => add({ type: 'collection', title, layout, items, ...extra })

// Photos
const tram = photo('Lisbon tram', 3)
const alfama = photo('Alfama rooftops', 7)
const belem = photo('Belém at dusk', 12)
const sintra = photo('Sintra hills', 19)
const cascais = photo('Cascais shore', 23)
const nazare = photo('Nazaré swell', 31)
const creek = photo('Creek in June', 37)
const ridge = photo('Ridge line', 41)
const dawn = photo('Dawn over the lake', 47)
const yard = photo('Back yard', 53)
const portugal = coll('Portugal', 'grid', [tram, alfama, belem, sintra, cascais, nazare], { accepts: 'photo' })
const home2 = coll('Near home', 'grid', [creek, ridge, dawn, yard], { accepts: 'photo' })
const photos = coll('Photos', 'grid', [portugal, home2], { accepts: 'collection', newType: 'album' })

// People
const lauren = note('Lauren Okafor', [
  p('Lauren works on the Lisbon trip with you.'),
  p('lauren@example.org'),
  p('Prefers mornings for calls.'),
], { person: true })
const sam = note('Sam Reyes', [p('Sam looks after the house while you travel.'), p('sam@example.org')], { person: true })
const jo = note('Jo Tremblay', [p('Jo is the clan administrator.'), p('jo@example.org')], { person: true })
const people = coll('People', 'list', [lauren, sam, jo], { accepts: 'note' })

// Music
const t1 = track('Harbor light', 214, 1)
const t2 = track('Slow ferry', 187, 2)
const t3 = track('Salt air', 241, 3)
const t4 = track('Night tram', 198, 4)
const t5 = track('Long table', 263, 5)
const a1 = coll('Harbor Light', 'list', [t1, t2, t3, t4, t5], { accepts: 'track', artist: 'Ada Marsh', seed: 61 })
const t6 = track('Paper boats', 176, 6)
const t7 = track('Low tide', 228, 7)
const t8 = track('Late bus', 205, 8)
const a2 = coll('Low Tide', 'list', [t6, t7, t8], { accepts: 'track', artist: 'The Quiet Hours', seed: 67 })
const t9 = track('Field recording 1', 142, 9)
const t10 = track('Field recording 2', 156, 10)
const a3 = coll('Field Recordings', 'list', [t9, t10], { accepts: 'track', artist: 'Ada Marsh', seed: 71 })
const music = coll('Music', 'grid', [a1, a2, a3], { accepts: 'collection', newType: 'album' })

// Movies
const m1 = movie('The Long Crossing', 5820, 73, [['Departure', 0], ['Open water', 1320], ['The storm', 2760], ['Landfall', 4380]])
const m2 = movie('Glass Harbor', 6240, 79, [['Arrival', 0], ['The letter', 1800], ['Winter', 3600], ['Glass', 5100]])
const m3 = movie('North Light', 5280, 83, [['First snow', 0], ['The cabin', 1500], ['Thaw', 3900]])
const m4 = movie('A Quiet Year', 6600, 89, [['Spring', 0], ['Summer', 1650], ['Autumn', 3300], ['Winter', 4950]])
const movies = coll('Movies', 'grid', [m1, m2, m3, m4], { accepts: 'movie' })

// Notes
const lisbon = note('Trip to Lisbon', [
  p('Trip to Lisbon', 'h'),
  p('Flights land Tuesday morning. The apartment is in Alfama, a short walk from the tram.'),
  ref(lauren),
  p('Book the tram tour', 'todo', { done: false }),
  p('Send Sam the house notes', 'todo', { done: true }),
  ref(tram),
  p('Places to see', 'h'),
  p('Belém for the tower and the pastries. Sintra if the weather holds. Cascais by train on the last day.'),
])
const reading = note('Reading list', [
  p('Reading list', 'h'),
  p('The Design of Everyday Things'),
  p('Finish the Kakoune design notes'),
  p('Read the Fuchsia Modular docs again', 'todo', { done: false }),
])
const review = note('Weekly review', [
  p('Weekly review', 'h'),
  p('What went well this week, and what to change next week.'),
  p('Call the dentist', 'todo', { done: false }),
  p('Try the keys on a real laptop', 'todo', { done: false }),
])
const keys = note('Keys', [
  p('Moving', 'h'),
  p('h j k l move one step. H J K L take a big step. g and G go to the start or end. / finds in this view.'),
  p('In and out', 'h'),
  p('Space activates: open, play or pause, tick. Esc or Caps Lock goes back one step. Shift+Space opens in a new tile.'),
  p('Selecting', 'h'),
  p('s marks one item. S starts a range, and moving extends it. a selects everything here.'),
  p('Verbs', 'h'),
  p('i edits. o makes a new item. x cuts, c copies, v pastes. z undoes, Z redoes. . repeats the last verb.'),
  p('Reaching everything else', 'h'),
  p('Each view lists its own one-key actions at the top right, such as r to rotate a photo. Tools you have taught the agent are behind t, then their key. f lists everything. ; opens the command bar, which is also where you talk to the agent.'),
  p('Apps', 'h'),
  p('From anywhere: n Notes, p Photos, m Music, w Movies, e People, C Calendar, M Messages, d Inbox, D Tools.'),
  p('Messages', 'h'),
  p('M opens Messages. o in a thread writes your draft, and Enter or Esc stops typing and keeps it. Enter never sends. A sends, the same key that approves a proposal. P sets who a thread is with. A sent message can be copied but not unsent.'),
  p('The agent', 'h'),
  p('; then a request asks the agent. What it wants to change waits in the Inbox: A approves, R rejects. Ask for the same thing twice and it offers a tool, which then sits behind t and runs without the agent.'),
  p('Calendar', 'h'),
  p('C opens the week. j k move half an hour, h l a day, H L a week, J K the next or previous event, u today. S selects time, then o makes an event from it. x and v move an event, c and v copy it, < and > change its length, y repeats it. Changing a repeating event changes every repeat.'),
  p('Tiles', 'h'),
  p('Alt+h j k l moves between tiles. Alt+q closes a tile.'),
])
const notes = coll('Notes', 'list', [lisbon, reading, review], { accepts: 'note', newType: 'note' })

// Calendar, around the week the store was made.
const week = weekStart(Date.now() / 1000)
const at = (day, hour) => addDays(week, day) + hour * 3600
const event = (title, day, hour, hours, items = [], extra = {}) => add({ type: 'event', title, start: at(day, hour), end: at(day, hour + hours), items, ...extra })
const calendar = coll('Calendar', 'calendar', [
  event('Trip planning with Lauren', 1, 10, 1, [ref(lauren), p('Agree the Sintra day.')]),
  event('Call the dentist', 1, 10.5, 0.5),
  event('Family dinner', 4, 18, 2, [ref(sam)]),
  event('Weekly review', 4, 9, 1, [ref(review)], { repeat: 'weekly' }),
], { id: 'calendar', accepts: 'event' })
// Messages
const said = (text, from, status, ago) => add({ type: 'message', text, from, status, at: Math.floor(Date.now() / 1000) - ago })
const thread = add({
  type: 'thread', title: 'Lauren Okafor', peer: '~sampel-palnet',
  items: [
    said('Did the tram tour open up for Tuesday?', '~sampel-palnet', 'received', 7200),
    said('Not yet. I’ll book it tonight.', 'me', 'delivered', 6900),
    said('Thanks. Mornings are best for me.', '~sampel-palnet', 'received', 6600),
  ],
})
const messages = coll('Messages', 'list', [thread], { id: 'messages', accepts: 'thread', newType: 'thread' })

const inbox = coll('Inbox', 'list', [], { id: 'inbox', fixed: true, query: { type: 'proposal', status: 'open' } })
const tools = coll('Tools', 'list', [], { id: 'tools', fixed: true, query: { type: 'tool' } })
const trash = coll('Trash', 'list', [], { id: 'trash', fixed: true, query: { $trashed: true } })
Object.entries({ n: notes, p: photos, m: music, w: movies, e: people, C: calendar, M: messages, d: inbox, D: tools }).forEach(([key, id]) => { s[id].launch = key })
add({ id: 'home', type: 'collection', title: 'Home', layout: 'grid', fixed: true, items: [notes, photos, music, movies, people, inbox, tools, keys, calendar, messages] })

export const seedStore = () => structuredClone(s)
