// Adds objects newer versions of the interface expect to a store seeded before them.
const url = process.env.LOCUS_URL ?? 'http://127.0.0.1:5180'
const s = await fetch(`${url}/store/~/v1/state`).then((r) => r.json())
const has = (id) => s.objects.some((o) => o.id === id)
const ops = []
if (!has('trash')) ops.push({ do: 'make', id: 'trash', type: 'collection', fields: { title: 'Trash', layout: 'list', items: [], fixed: true, query: { $trashed: true } } })
if (ops.length) console.log(await fetch(`${url}/store/~/v1/do`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ do: 'batch', intent: 'upgrade', ops }) }).then((r) => r.text()))
