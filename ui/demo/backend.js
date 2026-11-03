// Answers the interface's requests from inside the page, so the real App, sync
// and agent run unchanged with no server. Only the static media is fetched.
import { createAgent } from '../server/agent.mjs'
import { createShip } from './ship.js'
import { demoModel } from './model.js'
import { demoOps } from './seed.js'

export const KEY = 'locus-demo-1'

export function createBackend(saved, save) {
  const ship = createShip(saved, { save })
  if (!saved) ship.load(demoOps())
  const call = async (method, url, body) => {
    const [code, data] = ship.http(method, `/store/~/v1${url}`, body)
    if (code !== 200) throw new Error(data.error ?? 'refused')
    return method === 'POST' ? data : data
  }
  const agent = createAgent({ call, model: demoModel })
  return { ship, agent }
}

export function install() {
  let saved = null
  try { saved = JSON.parse(localStorage.getItem(KEY)) } catch {}
  let timer = null
  const save = (s) => {
    clearTimeout(timer)
    timer = setTimeout(() => { try { localStorage.setItem(KEY, JSON.stringify(s)) } catch {} }, 200)
  }
  const { ship, agent } = createBackend(saved, save)
  const json = (status, data) => new Response(JSON.stringify(data), { status, headers: { 'content-type': 'application/json' } })
  const real = window.fetch.bind(window)
  window.fetch = async (input, init = {}) => {
    const url = typeof input === 'string' ? input : input.url
    const method = init.method ?? 'GET'
    const body = init.body ? JSON.parse(init.body) : undefined
    if (url.startsWith('/store/')) return json(...ship.http(method, url, body))
    if (url === '/agent/ask') return json(200, await agent.ask(body))
    if (url === '/agent/tool') return json(200, await agent.tool(body))
    return real(input, init)
  }
}

export function reset() {
  localStorage.removeItem(KEY)
  location.reload()
}
