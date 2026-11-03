// The model the demo's agent calls: GPT-6 Luna through the Worker. When the
// Worker says no (a limit, or no network), the scripted model answers instead
// and says why, so the demo keeps working.
const WHY = {
  visitor: 'You’re asking quickly, so the scripted stand-in answered this one.',
  day: 'The live model’s daily limit is used up, so the scripted stand-in answered.',
  off: 'The live model is off, so the scripted stand-in answered.',
}

export function liveModel(fallback, fetcher = (...a) => fetch(...a)) {
  return async (prompt) => {
    let why = 'The live model couldn’t be reached, so the scripted stand-in answered.'
    try {
      const r = await fetcher('/api/agent', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ prompt }) })
      const d = await r.json()
      if (r.ok && typeof d.text === 'string') return { text: d.text, cost: d.cost ?? 0 }
      why = WHY[d.limit] ?? why
    } catch {}
    const r = await fallback(prompt)
    try {
      const p = JSON.parse(r.text)
      if (typeof p.reply === 'string') p.reply = `${why} ${p.reply}`
      return { ...r, text: JSON.stringify(p) }
    } catch { return r }
  }
}
