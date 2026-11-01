// Builds media-cache/manifest.json (safe to serve) and paths.json (private).
// Run it again whenever the libraries change.
import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { promisify } from 'node:util'
import { CACHE, HOST, SSH } from '../server/media.mjs'

const run = promisify(execFile)
const STOCK = path.join(import.meta.dirname, '..', 'media-stock')
const MOVIES = '/data/Media/Movies'
const idOf = (s) => createHash('sha1').update(s).digest('hex').slice(0, 16)
const paths = {}

// Stock photos from Wikimedia Commons, all CC0 or CC BY. Credits stay with them.
function photos() {
  const credits = JSON.parse(readFileSync(path.join(STOCK, 'credits.json'), 'utf8'))
  return Object.entries(credits).map(([title, items]) => {
    for (const p of items) paths[p.id] = { file: path.join(STOCK, `${p.id}.jpg`) }
    return { title, items }
  })
}

function chaptersOf(probe, dur) {
  const real = (probe.chapters ?? []).map((c, i) => [c.tags?.title || `Chapter ${i + 1}`, Number(c.start_time)])
  if (real.length > 1) return real
  const parts = [['Start', 0]]
  for (let t = 600, i = 2; t < dur; t += 600, i++) parts.push([`Part ${i}`, t])
  return parts
}

function tree(list) {
  const root = { title: 'Movies', folders: new Map(), movies: [] }
  for (const m of list) {
    let node = root
    for (const part of m.parts) {
      if (!node.folders.has(part)) node.folders.set(part, { title: part, folders: new Map(), movies: [] })
      node = node.folders.get(part)
    }
    const { id, title, dur, chapters } = m
    node.movies.push({ id, title, dur, chapters })
  }
  const plain = (n) => ({
    title: n.title,
    folders: [...n.folders.values()].map(plain).sort((a, b) => a.title.localeCompare(b.title)),
    movies: n.movies.sort((a, b) => a.title.localeCompare(b.title, undefined, { numeric: true })),
  })
  return plain(root)
}

// ffprobe runs on the media host, next to the files. Pulling them over the
// network to read their headers takes an hour; this takes minutes.
const PROBE = String.raw`find ${MOVIES} -type f \( -iname '*.mp4' -o -iname '*.m4v' \) -print0 |
while IFS= read -r -d '' f; do
  printf '\n@@FILE\t%s\t%s\n' "$(stat -c %s "$f")" "$f"
  timeout 30 ffprobe -v error -show_format -show_streams -show_chapters -of json "$f" || true
done`

function playable(p) {
  const v = p.streams?.find((s) => s.codec_type === 'video')
  const a = p.streams?.find((s) => s.codec_type === 'audio')
  return v?.codec_name === 'h264' && v.pix_fmt === 'yuv420p' && Number(p.format?.duration) > 0 && (!a || ['aac', 'mp3'].includes(a.codec_name))
}

async function movies() {
  const probing = run('ssh', [...SSH, HOST, 'nix shell nixpkgs#ffmpeg-headless -c bash -s'], { maxBuffer: 1 << 30 })
  probing.child.stdin.end(PROBE)
  const { stdout } = await probing
  const list = []
  let skipped = 0
  for (const chunk of stdout.split('\n@@FILE\t').slice(1)) {
    const nl = chunk.indexOf('\n')
    const [size, file] = chunk.slice(0, nl).split('\t')
    let p = {}
    try { p = JSON.parse(chunk.slice(nl + 1)) } catch { /* left unplayable */ }
    if (!playable(p)) { skipped++; continue }
    const dur = Number(p.format.duration)
    const id = idOf(file)
    paths[id] = { file, size: Number(size), dur }
    list.push({
      id, dur, chapters: chaptersOf(p, dur), parts: path.relative(MOVIES, file).split('/').slice(0, -1),
      title: path.parse(file).name.replace(/^-+|\s*- YouTube$/g, '').trim(),
    })
  }
  console.log(`movies: ${list.length} playable, ${skipped} in formats a browser can't play`)
  return tree(list)
}

mkdirSync(CACHE, { recursive: true })
const manifest = { albums: photos() }
console.log(`photos: ${Object.keys(paths).length} in ${manifest.albums.length} albums`)
writeFileSync(path.join(CACHE, 'manifest.json'), JSON.stringify({ ...manifest, movies: { folders: [], movies: [] } }))
writeFileSync(path.join(CACHE, 'paths.json'), JSON.stringify(paths))
manifest.movies = await movies()
writeFileSync(path.join(CACHE, 'manifest.json'), JSON.stringify(manifest))
writeFileSync(path.join(CACHE, 'paths.json'), JSON.stringify(paths))
console.log('done')
process.exit(0)
