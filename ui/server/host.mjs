// What runs beside the ship on a device: the interface, the media bytes, and
// the agent. It holds the ship's login, so the browser never needs it.
import http from 'node:http'
import fs from 'node:fs'
import path from 'node:path'
import { mediaRoutes } from './media.mjs'
import { claude, createAgent, stubModel } from './agent.mjs'

const PORT = Number(process.env.PORT ?? 5180)
const SHIP = process.env.LOCUS_SHIP ?? 'http://127.0.0.1:8081'
const CODE_FILE = process.env.LOCUS_CODE_FILE
const DIST = path.resolve(import.meta.dirname, '../dist')
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.svg': 'image/svg+xml', '.json': 'application/json', '.woff2': 'font/woff2' }

let cookie = ''
async function login() {
  const code = fs.readFileSync(CODE_FILE, 'utf8').trim()
  const r = await fetch(`${SHIP}/~/login`, { method: 'POST', body: new URLSearchParams({ password: code }), redirect: 'manual' })
  cookie = (r.headers.get('set-cookie') ?? '').split(';')[0]
  if (!cookie) throw new Error('the ship refused the login')
}

async function ship(url, init = {}) {
  if (!cookie) await login()
  const go = () => fetch(`${SHIP}${url}`, { ...init, headers: { ...init.headers, cookie } })
  let r = await go()
  if (r.status === 401 || r.status === 403) { await login(); r = await go() }
  return r
}

const call = async (method, url, body) => {
  const r = await ship(`/store/~/v1${url}`, method === 'POST' ? { method, headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) } : {})
  const d = await r.json()
  if (!r.ok) throw new Error(d.error ?? 'refused')
  return d
}

const model = process.env.LOCUS_LLM === 'stub' ? stubModel() : claude(process.env.LOCUS_MODEL)
const agent = createAgent({ call, model })
const media = mediaRoutes(PORT)

const body = (req) => new Promise((done) => {
  const chunks = []
  req.on('data', (c) => chunks.push(c))
  req.on('end', () => done(Buffer.concat(chunks)))
})
const send = (res, code, data) => { res.writeHead(code, { 'content-type': 'application/json' }); res.end(JSON.stringify(data)) }

http.createServer(async (req, res) => {
  const url = req.url
  try {
    if (url.startsWith('/store/')) {
      const payload = req.method === 'POST' ? await body(req) : undefined
      const r = await ship(url, { method: req.method, headers: payload ? { 'content-type': 'application/json' } : {}, body: payload })
      res.writeHead(r.status, { 'content-type': 'application/json' })
      return res.end(Buffer.from(await r.arrayBuffer()))
    }
    if (url === '/agent/ask') return send(res, 200, await agent.ask(JSON.parse(await body(req))))
    if (url === '/agent/tool') return send(res, 200, await agent.tool(JSON.parse(await body(req))))
    if (url === '/agent/stats') return send(res, 200, { ...agent.stats, calls: model.calls })
    if (url.startsWith('/media/')) return media(req, res, () => send(res, 404, {}))
    const file = path.join(DIST, url.split('?')[0] === '/' ? 'index.html' : url.split('?')[0])
    if (file.startsWith(DIST) && fs.existsSync(file) && fs.statSync(file).isFile()) {
      res.writeHead(200, { 'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream' })
      return fs.createReadStream(file).pipe(res)
    }
    send(res, 404, {})
  } catch (e) {
    console.error(url, e)
    send(res, 500, { say: `That failed: ${e.message}`, error: e.message })
  }
}).listen(PORT, process.env.HOST ?? '127.0.0.1', () => console.log(`locus on ${PORT}, ship ${SHIP}`))
