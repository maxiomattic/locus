// Serves real photos and movies to the prototype. Photos are read from this
// machine, movies from the media share on here-nor-there over SSH. Browsers
// only ever see ids; paths stay in media-cache/paths.json.
import { spawn } from 'node:child_process'
import { createReadStream, existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync } from 'node:fs'
import path from 'node:path'

export const ROOT = process.env.LOCUS_MEDIA ?? path.resolve(import.meta.dirname, '..')
export const CACHE = path.join(ROOT, 'media-cache')
export const HOST = 'here-nor-there'
export const SSH = ['-o', 'BatchMode=yes', '-o', 'ControlMaster=auto', '-o', 'ControlPath=/tmp/locus-media-%C', '-o', 'ControlPersist=600']
const CHUNK = 8 * 1024 * 1024
const SIZES = { thumb: 480, photo: 2000 }

const quote = (p) => `'${p.replace(/'/g, `'\\''`)}'`

export function sendRange(req, res, file, size, cap = CHUNK) {
  const m = /bytes=(\d+)-(\d*)/.exec(req.headers.range ?? '')
  const start = m ? Number(m[1]) : 0
  if (start >= size) { res.writeHead(416, { 'Content-Range': `bytes */${size}` }); return res.end() }
  const end = Math.min(m?.[2] ? Number(m[2]) : size - 1, start + cap - 1, size - 1)
  res.writeHead(206, {
    'Accept-Ranges': 'bytes', 'Content-Type': 'video/mp4', 'Content-Length': end - start + 1,
    'Content-Range': `bytes ${start}-${end}/${size}`,
  })
  const cmd = `tail -c +${start + 1} ${quote(file)} | head -c ${end - start + 1}`
  const ssh = spawn('ssh', [...SSH, HOST, cmd], { stdio: ['ignore', 'pipe', 'ignore'] })
  ssh.stdout.pipe(res)
  res.on('close', () => ssh.kill())
}

// One at a time per output file, and a few at once overall, so a screenful of
// thumbnails can't swamp the machine. Requests that go away leave the queue.
const waiting = []
const running = new Map()
let active = 0
function next() {
  while (active < 4 && waiting.length) {
    const job = waiting.shift()
    if (job.gone()) { job.done(false); continue }
    active++
    const tmp = `${job.out}.${process.pid}.tmp.jpg`
    const p = spawn('ffmpeg', ['-v', 'error', '-y', ...job.args, tmp], { stdio: 'ignore' })
    p.on('close', (code) => {
      active--
      if (code === 0) renameSync(tmp, job.out); else rmSync(tmp, { force: true })
      job.done(code === 0)
      next()
    })
  }
}
function render(out, args, gone) {
  if (existsSync(out)) return Promise.resolve(true)
  if (running.has(out)) return running.get(out)
  const job = new Promise((resolve) => {
    waiting.push({ out, args, gone, done: (ok) => { running.delete(out); resolve(ok) } })
    next()
  })
  running.set(out, job)
  return job
}

function sendJpeg(res, file) {
  res.writeHead(200, { 'Content-Type': 'image/jpeg', 'Content-Length': statSync(file).size, 'Cache-Control': 'max-age=86400' })
  createReadStream(file).pipe(res)
}

export function mediaRoutes(port) {
  const load = (name) => JSON.parse(readFileSync(path.join(CACHE, name), 'utf8'))
  mkdirSync(path.join(CACHE, 'img'), { recursive: true })
  return async (req, res, nextRoute) => {
    const url = new URL(req.url, 'http://x')
    if (url.pathname === '/media/manifest.json') {
      if (!existsSync(path.join(CACHE, 'manifest.json'))) { res.statusCode = 404; return res.end() }
      res.setHeader('Content-Type', 'application/json')
      return createReadStream(path.join(CACHE, 'manifest.json')).pipe(res)
    }
    const m = /^\/media\/(thumb|photo|poster|movie)\/([0-9a-f]+)$/.exec(url.pathname)
    if (!m) return nextRoute()
    const [, kind, id] = m
    const entry = load('paths.json')[id]
    if (!entry) { res.statusCode = 404; return res.end() }
    if (kind === 'movie') return sendRange(req, res, entry.file, entry.size, url.searchParams.has('full') ? Infinity : CHUNK)
    const out = path.join(CACHE, 'img', `${kind}-${id}.jpg`)
    const gone = () => res.destroyed
    const args = kind === 'poster'
      ? ['-ss', String(Math.floor(entry.dur * 0.1)), '-i', `http://127.0.0.1:${port}/media/movie/${id}?full`, '-frames:v', '1', '-vf', 'scale=480:-2', '-q:v', '5']
      : ['-i', entry.file, '-frames:v', '1', '-vf', `scale=w='min(${SIZES[kind]},iw)':h='min(${SIZES[kind]},ih)':force_original_aspect_ratio=decrease`, '-q:v', '4']
    const ok = await render(out, args, gone)
    if (res.destroyed) return
    if (!ok) { res.statusCode = 404; return res.end() }
    sendJpeg(res, out)
  }
}

export const mediaPlugin = () => ({
  name: 'locus-media',
  configureServer(server) {
    server.middlewares.use(mediaRoutes(server.config.server.port ?? 5180))
  },
})
