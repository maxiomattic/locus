// Static assets ignore Range, and a video can't seek without it. This serves a
// byte range of a movie and does nothing else; everything else never reaches it.
export default {
  async fetch(req, env) {
    const res = await env.ASSETS.fetch(new Request(req.url))
    const headers = new Headers(res.headers)
    headers.set('Accept-Ranges', 'bytes')
    const m = /^bytes=(\d+)-(\d*)$/.exec(req.headers.get('range') ?? '')
    if (!m || !res.ok) return new Response(res.body, { status: res.status, headers })
    const bytes = await res.arrayBuffer()
    const size = bytes.byteLength
    const start = Number(m[1])
    const end = Math.min(m[2] ? Number(m[2]) : size - 1, size - 1)
    if (start > end) { headers.set('Content-Range', `bytes */${size}`); return new Response(null, { status: 416, headers }) }
    headers.set('Content-Range', `bytes ${start}-${end}/${size}`)
    headers.delete('Content-Length')
    return new Response(new Uint8Array(bytes, start, end - start + 1), { status: 206, headers })
  },
}
