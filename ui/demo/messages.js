// Stands in for %messages in the public demo. Nothing leaves the page: a sent
// message is "delivered" by a timer and answered by an invented reply.
const REPLIES = [
  'This is a scripted reply. The demo has no network.',
  'Still scripted, but your message took the real send path.',
]

export function fakeMessages() {
  let id = Date.now()
  let replies = 0
  const all = []
  const stamp = () => {
    const d = new Date()
    return `~${d.getUTCFullYear()}.${d.getUTCMonth() + 1}.${d.getUTCDate()}..${d.getUTCHours()}.${d.getUTCMinutes()}.${d.getUTCSeconds()}`
  }
  const add = (peer, outgoing, body, status) => {
    const m = { id: ++id, peer, outgoing, body, status, sent: stamp(), group: null }
    all.push(m)
    return m
  }
  return async (method, url, body) => {
    if (method === 'POST' && url === '/messages') {
      const m = add(body.to, true, body.body, 'pending')
      setTimeout(() => { m.status = 'delivered' }, 800)
      setTimeout(() => add(body.to, false, REPLIES[replies++ % REPLIES.length], 'received'), 2500)
      return { ok: true, id: m.id }
    }
    if (url === '/conversations') {
      const peers = [...new Set(all.map((m) => m.peer))]
      return {
        conversations: peers.map((peer) => {
          const mine = all.filter((m) => m.peer === peer)
          return { peer, count: mine.length, unread: 0, last: mine.at(-1), blocked: false }
        }),
      }
    }
    if (url.startsWith('/conversations/') && method === 'GET') {
      return { messages: all.filter((m) => m.peer === url.split('/')[2]) }
    }
    return { ok: true }
  }
}
