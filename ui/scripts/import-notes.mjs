// Brings notes into the store: a `locus backup export` file, plus an Obsidian
// vault if given (the vault replaces the backup's notebook of the same name).
// Ids come from the source, so a rerun skips what is already there.
//   LOCUS_URL=… node scripts/import-notes.mjs <locus-backup.json> [<vault dir> [<notebook name>]]
import fs from 'node:fs'
import path from 'node:path'
import crypto from 'node:crypto'

const url = process.env.LOCUS_URL ?? 'http://127.0.0.1:5180'
const [backup, vault, vaultName = 'new'] = process.argv.slice(2)
const hash = (t) => crypto.createHash('sha256').update(t).digest('hex').slice(0, 16)

export const blocks = (md) => {
  const out = []
  for (const line of md.split('\n')) {
    if (!line.trim()) continue
    const h = line.match(/^#{1,6}\s+(.*)/)
    const t = line.match(/^\s*[-*]\s+\[([ xX])\]\s*(.*)/)
    if (h) out.push({ style: 'h', text: h[1] })
    else if (t) out.push({ style: 'todo', text: t[2], done: t[1] !== ' ' })
    else out.push({ style: 'p', text: line })
  }
  return out
}

// A source note: { key, title, md, folder: ['a', 'b'] }. Key is stable across runs.
const fromBackup = (nb) => {
  const folders = new Map(nb.folders.map((f) => [f.id, f]))
  const trail = (id) => {
    const f = folders.get(id)
    if (!f || f.parentFolderId == null) return []
    return [...trail(f.parentFolderId), f.name]
  }
  return nb.notes.map((n) => ({ key: `${nb.title}/${n.id}`, title: n.title, md: n.bodyMd ?? '', folder: trail(n.folderId) }))
}
const fromVault = (root) => {
  const out = []
  const walk = (dir, folder) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      if (e.name.startsWith('.')) continue
      const full = path.join(dir, e.name)
      if (e.isDirectory()) walk(full, [...folder, e.name])
      else if (e.name.endsWith('.md')) {
        out.push({ key: `${vaultName}/${path.relative(root, full)}`, title: e.name.slice(0, -3), md: fs.readFileSync(full, 'utf8'), folder })
      }
    }
  }
  walk(root, [])
  return out
}

const data = JSON.parse(fs.readFileSync(backup, 'utf8'))
const books = data.notebooks.filter((n) => !(vault && n.title === vaultName)).map((n) => ({ title: n.title, notes: fromBackup(n) }))
if (vault) books.unshift({ title: vaultName, notes: fromVault(vault) })

const state = await fetch(`${url}/store/~/v1/state`).then((r) => r.json())
const have = new Set(state.objects.map((o) => o.id))
const ops = []
const make = (id, type, fields) => { if (!have.has(id)) { have.add(id); ops.push({ do: 'make', id, type, fields }) } }

const bookIds = []
for (const book of books) {
  // Folder tree: path → { id, title, items }.
  const tree = new Map()
  const folderOf = (trail) => {
    const key = [book.title, ...trail].join('/')
    if (!tree.has(key)) {
      const node = { id: `c:${hash(key)}`, title: trail.at(-1) ?? book.title, items: [] }
      tree.set(key, node)
      if (trail.length) folderOf(trail.slice(0, -1)).items.unshift(node.id)
    }
    return tree.get(key)
  }
  const top = folderOf([])
  for (const n of book.notes) {
    const id = `n:${hash(n.key)}`
    const items = blocks(n.md).map((b, i) => {
      const bid = `${id}.${i}`
      make(bid, 'block', b)
      return bid
    })
    make(id, 'note', { title: n.title, items })
    folderOf(n.folder).items.push(id)
  }
  for (const f of tree.values()) make(f.id, 'collection', { title: f.title, layout: 'list', items: f.items, accepts: 'note', newType: 'note' })
  bookIds.push(top.id)
}

// The Home tile, not a notebook that happens to be called Notes.
const notes = state.objects.find((o) => o.fields?.launch === 'n')
if (notes) ops.push({ do: 'set', id: notes.id, base: notes.rev ?? 0, fields: { items: [...new Set([...bookIds, ...(notes.fields.items ?? [])])], accepts: 'collection' } })
const notesTotal = books.reduce((n, b) => n + b.notes.length, 0)
console.log(`${books.length} notebooks, ${notesTotal} notes, ${ops.length} ops`)

const SIZE = 200
for (let i = 0; i < ops.length; i += SIZE) {
  const r = await fetch(`${url}/store/~/v1/do`, {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ do: 'batch', intent: 'import notes', ops: ops.slice(i, i + SIZE) }),
  })
  if (!r.ok) { console.error(await r.text()); process.exit(1) }
  if ((i / SIZE) % 10 === 0) console.log(`${Math.min(i + SIZE, ops.length)} of ${ops.length}`)
}
