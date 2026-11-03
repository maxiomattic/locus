import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import {
  init, press, tick, point, setCols, frame, kind, items, titleOf, targetLabel,
  completions, range, clock, playerOf, visibleSize, viewActions, syncPlayer, CELLS,
  applyRemote, say, watch, pref, pointAt, unsend, startAt,
} from './engine.js'
import { addDays, dayName, dayStart, lanes, minuteOfDay, occurrences, weekStart, whenName } from './calendar.js'
import { describe } from './sync.js'
import { Bars, Film, Photo, Poster } from './Art.jsx'
import { ACCENT, Icon, iconFor } from './Icons.jsx'
import { Keys } from './Keys.jsx'

function toKey(e) {
  let key = e.key
  if (key === 'CapsLock') key = 'Escape'
  else if (key.length === 1 && /[a-zA-Z]/.test(key) && e.getModifierState('CapsLock')) {
    key = e.shiftKey ? key.toUpperCase() : key.toLowerCase()
  }
  if (e.altKey && /^Key[A-Z]$/.test(e.code)) key = e.code[3].toLowerCase()
  return { key, shift: e.shiftKey, alt: e.altKey, ctrl: e.ctrlKey || e.metaKey, now: Date.now() / 1000 }
}

export function App({ store, sync, extra }) {
  const [s, show] = useState(() => startAt({ ...init(store), extra: extra?.commands }, extra?.start))
  const [net, setNet] = useState({ state: 'connecting', pending: 0, ship: '' })
  const latest = useRef(s)
  const set = useCallback((fn) => {
    latest.current = fn(latest.current)
    show(latest.current)
  }, [])

  useEffect(() => {
    const down = (e) => {
      if (['Shift', 'Alt', 'Control', 'Meta'].includes(e.key)) return
      const k = toKey(e)
      if (k.ctrl || /^F\d+$/.test(k.key)) return
      set((p) => press(p, k))
      e.preventDefault()
    }
    window.addEventListener('keydown', down)
    return () => window.removeEventListener('keydown', down)
  }, [set])

  const link = useRef(null)
  useEffect(() => {
    if (!sync) return
    link.current = sync(() => latest.current, {
      remote: (objects, gone) => set((p) => applyRemote(p, objects, gone)),
      say: (msg, extra) => set((p) => say(p, msg, extra)),
      unsend: (id) => set((p) => unsend(p, id)),
      watch: (id) => set((p) => watch(p, id)),
      status: (net) => setNet(net),
      history: (history) => set((p) => ({ ...p, history })),
    })
    return () => link.current.stop()
  }, [set])

  // The engine is pure: what it wants done outside itself waits in s.out.
  useEffect(() => {
    if (!s.out.length) return
    const out = s.out
    set((p) => ({ ...p, out: [] }))
    out.forEach((e) => (e.do === 'page' ? extra?.run(e.what) : link.current?.send(e)))
  }, [s.out, set])

  useEffect(() => { link.current?.changed() }, [s.store])

  useEffect(() => {
    const id = setInterval(() => set((p) => tick(p, 0.1)), 100)
    return () => clearInterval(id)
  }, [set])

  return (
    <div className="shell">
      <Bar s={s} />
      <main className="tiles"><Layout s={s} t={s.layout} set={set} />{pref(s, 'help') && <Keys />}</main>
      <Status s={s} net={net} />
      {pref(s, 'notices') === 'toast' && s.mode === 'normal' && s.msg && <div className="toast">{s.msg}</div>}
    </div>
  )
}

function Bar({ s }) {
  const list = s.mode === 'bar' ? completions(s) : []
  const f = frame(s)
  return (
    <header className="bar">
      <span className="target">{targetLabel(s) || 'Empty'}</span>
      <span className="sep">›</span>
      {s.mode === 'bar' && (
        <span className={pref(s, 'bar') === 'float' ? 'line floating' : 'line'}>
          <span className="input">{s.bar.text}<i className="caret" /></span>
          {list.map((c, i) => <span key={c.label} className={i === s.bar.i ? 'pick on' : 'pick'}>{c.label}</span>)}
          {s.bar.text && !list.length && <span className="hint">Enter asks the agent</span>}
        </span>
      )}
      {s.mode === 'find' && <span className="line"><span className="input">/{f.filter}<i className="caret" /></span></span>}
      {s.mode === 'typing' && <span className="line hint">{s.typing?.note ? `${s.typing.note} ` : ''}Typing. Esc to stop.</span>}
      {s.mode === 'normal' && <span className="line hint">{(pref(s, 'notices') === 'line' && s.msg) || '; to type a command or ask the agent, ? for keys'}</span>}
    </header>
  )
}

function Status({ s, net }) {
  const [now, setNow] = useState(new Date())
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 20000)
    return () => clearInterval(id)
  }, [])
  const p = s.player
  return (
    <footer className="status">
      <span className="group"><b>Workspace 1</b></span>
      <span className="group">
        <span>{s.working ? `Agent: ${s.working}` : 'No agents working'}</span>
        <span>{net.state === 'offline' ? 'Ship unreachable' : net.pending ? `${net.pending} unconfirmed` : net.ship ? `${net.ship}, synced` : 'Connecting'}</span>
        <span>{s.held ? `Holding ${s.held.label}` : 'Holding nothing'}</span>
      </span>
      <span className="group">
        <span>{p.id ? `${p.playing ? 'Playing' : 'Paused'}: ${titleOf(s, p.id)}` : 'Nothing playing'}</span>
        <span>Volume {p.volume}</span>
        <span>{now.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}</span>
      </span>
    </footer>
  )
}

function Layout({ s, t, set }) {
  if (t.pane) return <Tile s={s} pane={t.pane} set={set} />
  return (
    <div className={`split split-${t.dir}`}>
      <Layout s={s} t={t.a} set={set} />
      <Layout s={s} t={t.b} set={set} />
    </div>
  )
}

function Tile({ s, pane, set }) {
  const active = s.active === pane
  const f = frame(s, pane)
  const k = kind(s, f)
  const ref = useRef(null)
  const cols = s.cols[pane]

  useLayoutEffect(() => {
    const el = ref.current
    const ro = new ResizeObserver(() => {
      const n = Math.max(1, Math.floor(el.clientWidth / 250))
      set((p) => setCols(p, pane, n))
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [pane, set])

  useLayoutEffect(() => {
    ref.current?.querySelector('.focus')?.scrollIntoView({ block: 'nearest' })
  })

  const pick = (i) => set((p) => point(p, pane, i))
  const props = { s, f, active, pick, cols }
  const crumbs = s.panes[pane].stack.map((x) => (x.view === 'info' ? `About ${titleOf(s, x.id)}` : x.view === 'history' ? 'History' : titleOf(s, x.id)))
  return (
    <section className={active ? 'tile active' : 'tile'}>
      <div className="tilehead">
        <h2 className="title" title={crumbs.join(' / ')}>{crumbs.at(-1) || 'Untitled'}</h2>
        {active && s.mode === 'normal' && (
          <span className="actions">
            {viewActions(s).map(([key, label, off]) => (
              <span key={key + label} className={off ? 'action off' : 'action'}>{key && <kbd>{key}</kbd>}{label}</span>
            ))}
          </span>
        )}
      </div>
      <div className="body" ref={ref}>
        {k === 'grid' && <Grid {...props} />}
        {k === 'list' && <List {...props} />}
        {k === 'calendar' && <Week {...props} pane={pane} set={set} />}
        {k === 'blocks' && <Note {...props} />}
        {k === 'thread' && <Thread {...props} />}
        {k === 'chars' && <Chars {...props} />}
        {k === 'photo' && <PhotoView {...props} />}
        {k === 'player' && <Player {...props} set={set} />}
        {k === 'info' && <Info {...props} />}
        {k === 'proposal' && <Proposal o={s.store[f.id]} s={s} />}
        {k === 'tool' && <ToolView o={s.store[f.id]} />}
        {k === 'history' && <History {...props} />}
      </div>
    </section>
  )
}

const itemClass = (f, active, i, id, base) =>
  `${base}${f.focus === i ? (active ? ' focus' : ' rest') : ''}${f.marks.includes(id) ? ' marked' : ''}`

function Caret({ text, at }) {
  return <>{text.slice(0, at)}<i className="caret" />{text.slice(at)}</>
}

function Title({ s, id, field = 'title' }) {
  const o = s.store[id]
  const t = s.typing
  if (t && t.objId === id && t.field === field) return <Caret text={o[field]} at={t.pos} />
  return o[field] || 'Untitled'
}

function Empty({ f, children }) {
  return <p className="empty">{f.filter ? `Nothing matches “${f.filter}”. Esc clears the search.` : children}</p>
}

function Card({ s, id }) {
  const o = s.store[id]
  const count = o.items ? `${o.items.length} ${o.items.length === 1 ? 'item' : 'items'}` : null
  if (o.type === 'photo') return <div className="frame"><Photo o={o} /><span className="cap">{o.title}</span></div>
  if (o.type === 'movie') return <Poster seed={o.seed} src={o.src} title={<Title s={s} id={id} />} sub={clock(o.dur)} />
  if (o.type === 'collection' && o.items[0] && s.store[o.items[0]].type === 'photo') {
    return <div className="frame"><Photo o={s.store[o.items[0]]} /><span className="cap"><Title s={s} id={id} /> <em>{count}</em></span></div>
  }
  if (o.artist) return <Poster seed={o.seed} title={<Title s={s} id={id} />} sub={o.artist} />
  const icon = o.type === 'collection' ? iconFor(o) : null
  return (
    <div className="door" style={icon ? { '--accent': ACCENT[o.launch] ?? 'var(--mist)' } : undefined}>
      {icon && <Icon name={icon} />}
      <strong><Title s={s} id={id} /></strong>{count && <span>{count}</span>}
    </div>
  )
}

// Albums run to thousands of photos, so only rows near the focus exist.
const REACH = 8

function Grid({ s, f, active, pick, cols = 1 }) {
  const list = items(s, f)
  const grid = useRef(null)
  const [rowH, setRowH] = useState(0)
  const row = Math.floor(f.focus / cols)
  const big = list.length > cols * (REACH * 2 + 1)
  const first = big ? Math.max(0, row - REACH) * cols : 0
  const last = big ? Math.min(list.length, (row + REACH + 1) * cols) : list.length
  useLayoutEffect(() => {
    const el = grid.current?.firstElementChild
    if (el) setRowH(el.offsetHeight + parseFloat(getComputedStyle(grid.current).rowGap))
  })
  const below = Math.ceil((list.length - last) / cols)
  return (
    <>
      <div ref={grid} className="grid" style={{
        gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
        paddingTop: (first / cols) * rowH, paddingBottom: below * rowH,
      }}>
        {list.slice(first, last).map((id, n) => {
          const i = first + n
          const key = s.store[id].launch
          return (
            <div key={id} className={itemClass(f, active, i, id, 'item')} onClick={() => pick(i)}>
              <Card s={s} id={id} />
              {f.id === 'home' && key && <kbd className="launch">{key}</kbd>}
            </div>
          )
        })}
      </div>
      {!list.length && <Empty f={f}>This is empty. Press o to make something, or v to paste.</Empty>}
    </>
  )
}

function Clashes({ o }) {
  return (
    <>
      <h3>Two versions</h3>
      <dl>{o.$clashes.map((c) => <div key={c.seq + c.field}><dt>{c.actor}</dt><dd>{String(c.theirs)}</dd></div>)}</dl>
    </>
  )
}

function History({ s, f, active, pick }) {
  if (!s.history.length) return <Empty f={f}>Nothing yet.</Empty>
  return (
    <div className="list">
      {s.history.map((e, i) => (
        <div key={e.seq} className={itemClass(f, active, i, e.seq, 'row')} onClick={() => pick(i)}>
          <span className="grow">{e.intent || e.ops.map((op) => describe(op, s.store)).join(', ')}</span>
          <span className="dim">{e.actor} · {e.ops.length} · {new Date(e.at * 1000).toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })}</span>
        </div>
      ))}
    </div>
  )
}

function Proposal({ o, s }) {
  return (
    <article className="info">
      <h3>{o.intent}</h3>
      <dl>
        <div><dt>From</dt><dd>{o.actor}</dd></div>
        <div><dt>Status</dt><dd>{o.status}</dd></div>
      </dl>
      <h3>What would change</h3>
      <ul className="changes">{(o.ops ?? []).map((op, i) => <li key={i}>{describe(op, s.store)}</li>)}</ul>
    </article>
  )
}

function ToolView({ o }) {
  return (
    <article className="info">
      <dl>
        <div><dt>Name</dt><dd>{o.name}</dd></div>
        <div><dt>Key</dt><dd>{o.key ? <kbd>{o.key}</kbd> : 'By name, from the command bar'}</dd></div>
        <div><dt>Works on</dt><dd>{o.appliesTo}</dd></div>
        <div><dt>Runs</dt><dd>{o.uses ?? 0}, with no model calls</dd></div>
        <div><dt>Version</dt><dd>{o.version ?? 1}</dd></div>
        <div><dt>Can do</dt><dd>{(o.can ?? []).join(', ')}</dd></div>
      </dl>
      <h3>Program</h3>
      <pre className="program">{JSON.stringify(o.program, null, 1)}</pre>
    </article>
  )
}

function Row({ s, id }) {
  const o = s.store[id]
  if (o.type === 'thread') {
    const last = o.items.map((x) => s.store[x]).filter(Boolean).at(-1)
    return <><span className="grow"><Title s={s} id={id} /></span><span className="grow dim">{last?.text ?? ''}</span><span className="dim">{o.unread ? `${o.unread} new` : o.peer || 'No ship yet'}</span></>
  }
  if (o.type === 'proposal') return <><span className="grow">{o.intent}</span><span className="dim">{(o.ops ?? []).length} changes</span></>
  if (o.type === 'tool') return <><span className="grow">{o.name}</span><span className="dim">{o.key && <kbd>{o.key}</kbd>} {o.uses ?? 0} runs</span></>
  if (o.type === 'track') return <><span className="grow">{o.title}</span><span className="dim">{s.player.id === id ? (s.player.playing ? 'Playing' : 'Paused') : clock(o.dur)}</span></>
  const sub = o.type === 'note' ? `${o.items.length} ${o.items.length === 1 ? 'block' : 'blocks'}` : ''
  return <><span className="grow"><Title s={s} id={id} /></span><span className="dim">{sub}</span></>
}

function List({ s, f, active, pick }) {
  const list = items(s, f)
  return (
    <div className="list">
      {list.map((id, i) => (
        <div key={id} className={itemClass(f, active, i, id, 'row')} onClick={() => pick(i)}><Row s={s} id={id} /></div>
      ))}
      {!list.length && <Empty f={f}>This is empty. Press o to make something, or v to paste.</Empty>}
    </div>
  )
}

const SLOTS = Array.from({ length: 48 }, (_, i) => i)
const hourName = (i) => new Date(2000, 0, 1, i / 2).toLocaleTimeString([], { hour: 'numeric' })

function Week({ s, f, active, pane, set }) {
  const o = s.store[f.id]
  const events = o.items.map((id) => s.store[id]).filter((e) => e?.type === 'event')
  const shown = new Set(items(s, f))
  const start = weekStart(f.at)
  const days = Array.from({ length: 7 }, (_, i) => addDays(start, i))
  const all = occurrences(events, start, addDays(start, 7))
  const cursorDay = dayStart(f.at)
  const now = Date.now() / 1000
  const r = range(s, f)
  const on = s.mode === 'normal' ? targetsHere(s, f, all) : null
  // Part of a time span that falls within one day, in slots.
  const span = (day, a, b) => {
    const next = addDays(day, 1)
    if (a >= next || b <= day) return null
    return { top: a <= day ? 0 : minuteOfDay(a) / 30, bottom: b >= next ? 48 : minuteOfDay(b) / 30 || 48 }
  }
  return (
    <div className="week">
      <div className="weekhead">
        <span />
        {days.map((d) => <span key={d} className={dayStart(now) === d ? 'today' : ''}>{dayName(d)}</span>)}
      </div>
      <div className="weekbody">
        <div className="hours">{SLOTS.filter((i) => i % 2 === 0).map((i) => <span key={i}>{hourName(i)}</span>)}</div>
        {days.map((d) => {
          const segs = lanes(all.flatMap((e) => {
            const p = span(d, e.start, e.end)
            return p ? [{ ...p, id: e.id, from: e.start, to: e.end }] : []
          }))
          const sel = r && span(d, r.from, r.to)
          return (
            <div key={d} className="day" onClick={(e) => {
              const rows = (e.clientY - e.currentTarget.getBoundingClientRect().top) / e.currentTarget.offsetHeight * 48
              const t = new Date(d * 1000)
              t.setMinutes(Math.floor(rows) * 30)
              set((p) => pointAt(p, pane, t / 1000))
            }}>
              {segs.map((g) => {
                const pct = 100 / g.lanes
                const name = `event${f.marks.includes(g.id) ? ' marked' : ''}${on === g.id ? ' on' : ''}${f.filter && !shown.has(g.id) ? ' faint' : ''}`
                return (
                  <div key={g.id + g.from} className={name} title={whenName(g.from, g.to)} style={{
                    top: `${(g.top / 48) * 100}%`, height: `${((g.bottom - g.top) / 48) * 100}%`,
                    left: `${g.lane * pct}%`, width: `${pct}%`,
                  }} onClick={(e) => {
                    e.stopPropagation()
                    set((p) => pointAt(p, pane, Math.max(g.from, d), g.id))
                  }}>
                    <Title s={s} id={g.id} />
                    {s.store[g.id].repeat && <span className="dim"> {s.store[g.id].repeat}</span>}
                  </div>
                )
              })}
              {sel && <div className="span" style={{ top: `${(sel.top / 48) * 100}%`, height: `${((sel.bottom - sel.top) / 48) * 100}%` }} />}
              {dayStart(now) === d && <div className="nowline" style={{ top: `${(minuteOfDay(now) / 1440) * 100}%` }} />}
              {cursorDay === d && <div className={active ? 'slotcursor focus' : 'slotcursor rest'} style={{ top: `${(minuteOfDay(f.at) / 1440) * 100}%` }} />}
            </div>
          )
        })}
      </div>
    </div>
  )
}

function targetsHere(s, f, all) {
  const hits = all.filter((e) => e.start < f.at + 1800 && e.end > f.at)
  return (hits.find((e) => e.id === f.sel) ?? hits[0])?.id ?? null
}

function Block({ s, id }) {
  const o = s.store[id]
  if (o.ref) {
    const r = s.store[o.ref]
    if (r.type === 'event') return <p className="link">{r.title}, {whenName(r.start, r.end)}</p>
    if (r.type === 'photo') return <figure className="embed"><Photo o={r} crop={o.crop} /><figcaption>{r.title}{o.crop ? ' (part)' : ''}</figcaption></figure>
    if (r.type === 'message') return <p className="link">{r.text}</p>
    if (o.clip) return <p className="link">{r.title}, {clock(o.clip.from)} to {clock(o.clip.to)}</p>
    return <p className="link">{r.title}</p>
  }
  if (o.style === 'h') return <h3><Title s={s} id={id} field="text" /></h3>
  if (o.style === 'todo') {
    return <p className={o.done ? 'todo done' : 'todo'}><span className="box">{o.done ? '✓' : ''}</span><span><Title s={s} id={id} field="text" /></span></p>
  }
  return <p><Title s={s} id={id} field="text" /></p>
}

function Note({ s, f, active, pick }) {
  const list = items(s, f)
  return (
    <article className="note">
      {list.map((id, i) => (
        <div key={id} className={itemClass(f, active, i, id, 'block')} onClick={() => pick(i)}><Block s={s} id={id} /></div>
      ))}
      {!list.length && <Empty f={f}>This note is empty. Press o to start writing.</Empty>}
    </article>
  )
}

function Chars({ s, f, active }) {
  const o = s.store[f.id]
  const t = s.typing
  if (t && t.objId === f.id) return <article className="note"><p className="chars"><Caret text={o.text} at={t.pos} /></p></article>
  const r = range(s, f)
  const a = r ? r.from : f.cursor
  const b = r ? r.to : f.cursor + 1
  return (
    <article className="note">
      <p className="chars">
        {o.text.slice(0, a)}
        <span className={r ? 'sel' : active ? 'cur' : 'cur rest'}>{o.text.slice(a, b) || ' '}</span>
        {o.text.slice(b)}
      </p>
    </article>
  )
}

const SHOWN = { draft: 'Draft', sending: 'Sending', pending: 'Sent', delivered: 'Delivered', failed: 'Failed, press A to send again' }
const stamp = (at) => new Date(at * 1000).toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })

function Message({ s, id }) {
  const o = s.store[id]
  const mine = o.from === 'me'
  const state = o.$sending ? 'Sending' : mine ? SHOWN[o.status] : null
  const typing = s.typing?.objId === id
  return (
    <div className={`msg ${mine ? 'mine' : 'theirs'} ${o.status}`}>
      <p>{typing ? <Caret text={o.text} at={s.typing.pos} /> : o.text}</p>
      <span className="dim">{[o.at && stamp(o.at), state].filter(Boolean).join(' · ')}</span>
    </div>
  )
}

function Thread({ s, f, active, pick }) {
  const o = s.store[f.id]
  const t = s.typing
  const list = items(s, f)
  return (
    <div className="thread">
      <p className="dim">{t?.objId === f.id && t.field === 'peer' ? <>With <Caret text={o.peer ?? ''} at={t.pos} /></> : o.peer ? `With ${o.peer}` : 'No ship yet. Press P to set one.'}</p>
      {list.map((id, i) => (
        <div key={id} className={itemClass(f, active, i, id, 'turn')} onClick={() => pick(i)}><Message s={s} id={id} /></div>
      ))}
      {!list.length && <Empty f={f}>Nothing here yet. Press o to write.</Empty>}
    </div>
  )
}

function PhotoView({ s, f, active }) {
  const o = s.store[f.id]
  const r = range(s, f)
  const pct = (a, of) => `${(a / of) * 100}%`
  const box = (x, y, w, h) => ({ left: pct(x, CELLS.x), top: pct(y, CELLS.y), width: pct(w, CELLS.x), height: pct(h, CELLS.y) })
  const size = visibleSize(o)
  const ratio = size.w / size.h
  return (
    <div className="stage">
      <div className="shot" style={{ '--ratio': ratio }}>
        <Photo o={o} full />
        {r && <div className="region" style={box(r.x, r.y, r.w, r.h)} />}
      </div>
    </div>
  )
}

function Video({ o, p, set }) {
  const ref = useRef(null)
  const [note, setNote] = useState('Loading…')
  const src = `/media/movie/${o.src}`
  useEffect(() => {
    const v = ref.current
    if (Math.abs(v.currentTime - p.t) > 1.5) v.currentTime = p.t
  }, [p.t])
  useEffect(() => { ref.current.volume = p.volume / 100 }, [p.volume])
  useEffect(() => {
    if (p.playing) {
      ref.current.play().catch((e) => {
        if (e.name === 'NotAllowedError') setNote('The browser blocked playback. Press Space to try again.')
      })
    }
    else ref.current.pause()
  }, [p.playing])
  const report = (e, ended) => {
    const v = e.currentTarget
    if (!v.seeking) set((q) => syncPlayer(q, o.id, v.currentTime, ended))
  }
  return (
    <>
      <video ref={ref} className="video" src={src} preload="auto" playsInline
        onLoadedMetadata={(e) => { e.currentTarget.currentTime = p.t }}
        onWaiting={() => setNote('Loading…')} onPlaying={() => setNote('')} onCanPlay={() => setNote('')}
        onError={() => setNote('This movie could not be loaded.')}
        onTimeUpdate={(e) => report(e, false)} onEnded={(e) => report(e, true)} />
      {note && <span className="loading">{note}</span>}
    </>
  )
}

function Player({ s, f, set }) {
  const o = s.store[f.id]
  const p = playerOf(s, f)
  const r = range(s, f)
  const now = (p.t / o.dur) * 100
  const movie = o.type === 'movie'
  const chapter = movie ? o.chapters[o.chapters.findLastIndex((c) => c[1] <= p.t)][0] : null
  return (
    <div className="player">
      <div className="screen">
        {o.src ? <Video o={o} p={p} set={set} /> : movie ? <Film o={o} t={p.t} /> : <Bars seed={o.seed} t={p.t} playing={p.playing} />}
        {!p.playing && <span className="paused">Paused</span>}
      </div>
      <div className="meta">
        <strong>{o.title}</strong>
        <span className="dim">{movie ? chapter : 'Playback is simulated'}</span>
      </div>
      <div className="time">
        <span>{clock(p.t)}</span>
        <div className="track">
          {movie && o.chapters.map((c) => <i key={c[0]} className="tick" style={{ left: `${(c[1] / o.dur) * 100}%` }} />)}
          {r && <i className="clip" style={{ left: `${(r.from / o.dur) * 100}%`, width: `${((r.to - r.from) / o.dur) * 100}%` }} />}
          <i className="fill" style={{ width: `${now}%` }} />
          <i className="head" style={{ left: `${now}%` }} />
        </div>
        <span>{clock(o.dur)}</span>
      </div>
    </div>
  )
}

function Info({ s, f, active, pick }) {
  const o = s.store[f.id]
  const list = items(s, f)
  const type = { photo: 'Photo', note: o.person ? 'Person' : 'Note', event: 'Event', track: 'Track', movie: 'Movie', collection: 'Collection', block: 'Block', thread: 'Thread' }[o.type]
  const rows = [['Kind', type]]
  if (o.dur) rows.push(['Length', clock(o.dur)])
  if (o.type === 'event') rows.push(['When', whenName(o.start, o.end)], ['Repeats', o.repeat ?? 'No'])
  if (o.type === 'photo') rows.push(['Rotation', `${o.rot} degrees`], ['Cropped', o.crop ? 'Yes' : 'No'])
  if (o.author) rows.push(['Photographer', o.author], ['License', o.license])
  if (o.items) rows.push(['Contains', `${o.items.length}`])
  return (
    <div className="info">
      <dl>{rows.map(([a, b]) => <div key={a}><dt>{a}</dt><dd>{b}</dd></div>)}</dl>
      {o.$clashes?.length > 0 && <Clashes o={o} />}
      <h3>Linked from</h3>
      <div className="list">
        {list.map((id, i) => (
          <div key={id} className={itemClass(f, active, i, id, 'row')} onClick={() => pick(i)}><span className="grow">{titleOf(s, id)}</span></div>
        ))}
        {!list.length && <p className="empty">Nothing links here yet.</p>}
      </div>
    </div>
  )
}
