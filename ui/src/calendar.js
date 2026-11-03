// Time, with no UI in it. Times are Unix seconds, and everything that means
// "the same time on another day" goes through the wall clock, so daylight saving
// never moves a meeting.
export const SLOT = 1800
const DAY = 86400

export const dayStart = (t) => { const d = new Date(t * 1000); d.setHours(0, 0, 0, 0); return d / 1000 }
export const addDays = (t, n) => { const d = new Date(t * 1000); d.setDate(d.getDate() + n); return d / 1000 }
export const slotOf = (t) => { const d = new Date(t * 1000); d.setMinutes(d.getMinutes() < 30 ? 0 : 30, 0, 0); return d / 1000 }
export const weekStart = (t) => { const d = new Date(t * 1000); return addDays(dayStart(t), -((d.getDay() + 6) % 7)) }
export const minuteOfDay = (t) => { const d = new Date(t * 1000); return d.getHours() * 60 + d.getMinutes() }

// Moves t by the wall-clock distance from one moment to another.
export function retime(t, from, to) {
  const days = Math.round((dayStart(to) - dayStart(from)) / DAY)
  const d = new Date(addDays(t, days) * 1000)
  d.setMinutes(d.getMinutes() + minuteOfDay(to) - minuteOfDay(from))
  return d / 1000
}

// Every time one event happens within [from, to).
export function occursIn(o, from, to) {
  const len = o.end - o.start
  if (!o.repeat) return o.start < to && o.end > from ? [{ id: o.id, start: o.start, end: o.end }] : []
  const step = o.repeat === 'weekly' ? 7 : 1
  const out = []
  let n = Math.max(0, Math.floor((from - len - o.start) / DAY / step) - 1)
  for (let start = addDays(o.start, n * step); start < to; start = addDays(o.start, ++n * step)) {
    if (start + len > from) out.push({ id: o.id, start, end: start + len })
  }
  return out
}

export function occurrences(events, from, to) {
  return events.flatMap((o) => occursIn(o, from, to))
    .sort((a, b) => a.start - b.start || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
}

// Side-by-side columns for what overlaps within a day.
export function lanes(segs) {
  const out = []
  let group = []
  let reach = -Infinity
  const close = () => {
    const ends = []
    for (const g of group) {
      let lane = ends.findIndex((e) => e <= g.top)
      if (lane < 0) lane = ends.length
      ends[lane] = g.bottom
      g.lane = lane
    }
    for (const g of group) out.push({ ...g, lanes: ends.length })
    group = []
  }
  for (const g of [...segs].sort((a, b) => a.top - b.top || a.bottom - b.bottom)) {
    if (g.top >= reach) { close(); reach = -Infinity }
    group.push({ ...g })
    reach = Math.max(reach, g.bottom)
  }
  close()
  return out
}

export const dayName = (t) => new Date(t * 1000).toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' })
export const timeName = (t) => new Date(t * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
export function whenName(a, b) {
  return dayStart(a) === dayStart(b - 1)
    ? `${dayName(a)}, ${timeName(a)} to ${timeName(b)}`
    : `${dayName(a)}, ${timeName(a)} to ${dayName(b)}, ${timeName(b)}`
}
