// The demo's live agent: the Worker forwards the agent's own prompts to GPT-6
// Luna on OpenRouter. The key stays a Worker secret, so the page never sees it.
// Anyone can reach this URL, so it only takes prompts shaped like the agent's,
// and per-visitor and daily limits keep a stranger from spending much.
export const MODEL = 'openai/gpt-6-luna'
export const PREFIXES = [
  'You act for the owner of a personal computer.',
  'Write a deterministic program for a task that was done twice by hand.',
]
export const LIMITS = { promptChars: 40000, outputTokens: 2000, dailyUsd: 1, dailyCalls: 400 }

const json = (status, data) => new Response(JSON.stringify(data), { status, headers: { 'content-type': 'application/json' } })

export async function agentProxy(req, env, fetcher = fetch, today = new Date().toISOString().slice(0, 10)) {
  if (req.method !== 'POST') return json(405, { error: 'POST only' })
  if (!env.OPENROUTER_API_KEY) return json(503, { error: 'no model configured', limit: 'off' })
  let prompt = ''
  try { prompt = String((await req.json()).prompt ?? '') } catch { return json(400, { error: 'bad request' }) }
  if (!PREFIXES.some((p) => prompt.startsWith(p)) || prompt.length > LIMITS.promptChars) return json(400, { error: 'not an agent prompt' })

  const ip = req.headers.get('cf-connecting-ip') ?? 'unknown'
  if (env.PER_VISITOR && !(await env.PER_VISITOR.limit({ key: ip })).success) return json(429, { error: 'too fast', limit: 'visitor' })
  const day = `day:${today}`
  const spent = env.BUDGET ? JSON.parse((await env.BUDGET.get(day)) ?? '{"usd":0,"calls":0}') : { usd: 0, calls: 0 }
  if (spent.usd >= LIMITS.dailyUsd || spent.calls >= LIMITS.dailyCalls) return json(429, { error: 'daily limit reached', limit: 'day' })

  const r = await fetcher('https://openrouter.ai/api/v1/chat/completions', {
    method: 'POST',
    headers: {
      authorization: `Bearer ${env.OPENROUTER_API_KEY}`,
      'content-type': 'application/json',
      'http-referer': 'https://locus.careride.workers.dev',
      'x-title': 'Locus demo',
    },
    body: JSON.stringify({
      model: MODEL,
      reasoning: { effort: 'low' },
      max_tokens: LIMITS.outputTokens,
      response_format: { type: 'json_object' },
      messages: [{ role: 'user', content: prompt }],
    }),
  })
  const d = await r.json().catch(() => ({}))
  const cost = Number(d.usage?.cost ?? 0)
  if (env.BUDGET) await env.BUDGET.put(day, JSON.stringify({ usd: spent.usd + cost, calls: spent.calls + 1 }), { expirationTtl: 60 * 60 * 48 })
  const text = d.choices?.[0]?.message?.content
  if (!r.ok || typeof text !== 'string') return json(502, { error: d.error?.message ?? 'the model gave no answer' })
  return json(200, { text, cost })
}
