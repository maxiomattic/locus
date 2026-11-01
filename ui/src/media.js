// Swaps the sample photos and movies for the real libraries.
export function withMedia(store, m) {
  const coll = (title) => Object.values(store).find((o) => o.type === 'collection' && o.title === title)
  const fake = new Set(Object.values(store).filter((o) => o.type === 'photo' || o.type === 'movie').map((o) => o.id))
  for (const o of Object.values(store)) {
    if (o.type === 'block' && fake.has(o.ref)) delete store[o.id]
    if (o.type === 'collection' && o.items.some((id) => store[id]?.type === 'photo')) delete store[o.id]
  }
  for (const id of fake) delete store[id]
  for (const o of Object.values(store)) if (o.items) o.items = o.items.filter((id) => store[id])

  const add = (o) => { store[o.id] = o; return o.id }
  coll('Photos').items = m.albums.map((a) => add({
    id: `album-${a.title}`, type: 'collection', title: a.title, layout: 'grid', accepts: 'photo',
    items: a.items.map((p) => add({ id: `p-${p.id}`, type: 'photo', title: p.title, src: p.id, w: p.w, h: p.h, author: p.author, license: p.license, seed: 0, rot: 0, crop: null })),
  }))
  const folder = (n, id) => [
    ...n.movies.map((v) => add({ id: `m-${v.id}`, type: 'movie', title: v.title, src: v.id, dur: v.dur, chapters: v.chapters, seed: 0 })),
    ...n.folders.map((c) => add({ id: `${id}/${c.title}`, type: 'collection', title: c.title, layout: 'grid', accepts: 'movie', items: folder(c, `${id}/${c.title}`) })),
  ]
  coll('Movies').items = folder(m.movies, 'movies')
  return store
}
