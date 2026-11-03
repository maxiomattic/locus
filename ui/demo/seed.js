// The demo's contents. Everything here is invented; the photos and movies are
// CC0 or public domain (demo/media.json, with their sources on credits.html).
import { seedStore } from '../src/data.js'
import { withMedia } from '../src/media.js'
import media from './media.json'
import { messageSetup } from '../server/messages.mjs'

const welcomeText = [
  ['h', 'Welcome to Locus'],
  ['p', 'Locus is a personal computer that keeps everything as one kind of thing: objects in a store you own. Notes, photos, music, movies and people are all objects, and apps are only ways of looking at them.'],
  ['p', 'This is a working copy running entirely in your browser, with invented sample data. Only what you ask the agent leaves this page, and Reset at the top puts it back.'],
  ['h', 'A short tour'],
  ['p', 'Locus is built for the keyboard. On a phone, use the buttons at the bottom: you can browse everything, but asking the agent needs a keyboard. Press Esc to go back, then h j k l to move and Space to open. Press f any time to see what you can do here.'],
  ['todo', 'Press p for Photos, open an album, and press r on a photo to rotate it', false],
  ['todo', 'Press z to undo it', false],
  ['todo', 'Select a photo with s, press ; and ask “rotate this”. The change waits in the Inbox: press d, Space to open it, then A to approve', false],
  ['todo', 'Do the same thing twice and the agent offers a tool, which then runs with no agent at all (press D to list them; on a photo they show up as one-key actions)', false],
  ['p', 'The agent is a live model, GPT-6 Luna. A request sends the agent’s prompt (your request, what you selected and the names of notes and collections) to OpenRouter. Past a small limit, a short script answers instead.'],
]

const extras = [
  ['Garden plan', [
    ['h', 'Garden plan'],
    ['p', 'Tomatoes along the south fence, beans on the trellis, and herbs by the kitchen door.'],
    ['todo', 'Order seed potatoes', true],
    ['todo', 'Fix the gate latch', false],
    ['todo', 'Turn the compost', false],
  ]],
  ['Sourdough', [
    ['h', 'Sourdough'],
    ['p', '500 g flour, 350 g water, 100 g starter, 10 g salt. Mix, rest thirty minutes, fold every half hour for two hours, then shape.'],
    ['p', 'Overnight in the fridge. Bake in a covered pot at 240 °C for 25 minutes, then uncovered for 20.'],
  ]],
  ['Book club picks', [
    ['h', 'Book club picks'],
    ['p', 'Public-domain novels, so everyone can read them for free.'],
    ['todo', 'Middlemarch', true],
    ['todo', 'The Time Machine', false],
    ['todo', 'Twenty Thousand Leagues Under the Sea', false],
  ]],
]

export function demoOps() {
  const store = withMedia(seedStore(), media)
  const notes = Object.values(store).find((o) => o.launch === 'n')
  const home = store.home
  let n = 0
  const block = ([style, text, done]) => {
    const id = `demo-b${++n}`
    store[id] = { id, type: 'block', style, text, ...(style === 'todo' ? { done } : {}) }
    return id
  }
  const note = (id, title, lines) => { store[id] = { id, type: 'note', title, items: lines.map(block) }; return id }
  notes.items.push(...extras.map(([title, lines], i) => note(`demo-note${i}`, title, lines)))
  home.items.unshift(note('demo-welcome', 'Welcome', welcomeText))

  const ops = Object.values(store).map(({ id, type, ...fields }) => ({ do: 'make', id, type, fields }))
  const kind = (fields) => ({ do: 'make', id: `type:${fields.name}`, type: 'type', fields })
  ops.push(
    kind({ name: 'block', fields: { text: 'text', style: 'text', done: 'bool', ref: 'ref' } }),
    kind({ name: 'note', fields: { title: 'text', items: 'refs' } }),
    kind({ name: 'event', fields: { title: 'text', start: 'num', end: 'num', repeat: 'text', items: 'refs' } }),
    kind({ name: 'photo', fields: { title: 'text', rot: 'num', src: 'blob' } }),
    kind({ name: 'collection', fields: { title: 'text', items: 'refs' } }),
    ...messageSetup(kind),
    { do: 'make', id: 'grant:host-runs', type: 'grant', fields: { actor: 'agent:host', ops: ['make', 'set'], types: ['run'] } },
    { do: 'make', id: 'grant:host-tools', type: 'grant', fields: { actor: 'agent:host', ops: ['set'], types: ['tool'] } },
  )
  return ops
}
