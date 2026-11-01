import { BASE, visibleCrop, visibleSize } from './engine.js'

// Pictures are drawn from a seed, so the prototype needs no image files.
const rng = (seed) => {
  let x = seed * 9301 + 49297
  return () => { x = (x * 9301 + 49297) % 233280; return x / 233280 }
}

function Scene({ seed }) {
  const r = rng(seed)
  const hue = Math.floor(r() * 360)
  const sky = [`hsl(${hue} 62% ${28 + r() * 16}%)`, `hsl(${(hue + 44) % 360} 74% ${62 + r() * 12}%)`]
  const ridges = [0, 1, 2, 3].map((i) => {
    const base = 380 + i * 105
    let d = `M0 ${base}`
    for (let x = 0; x <= BASE.w; x += 100) d += ` L${x} ${base + (r() - 0.5) * (150 - i * 18)}`
    return { d: `${d} L${BASE.w} ${BASE.h} L0 ${BASE.h} Z`, fill: `hsl(${(hue + 170 + i * 8) % 360} 34% ${40 - i * 8}%)` }
  })
  const g = `sky${seed}`
  return (
    <g>
      <defs>
        <linearGradient id={g} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={sky[0]} />
          <stop offset="1" stopColor={sky[1]} />
        </linearGradient>
      </defs>
      <rect width={BASE.w} height={BASE.h} fill={`url(#${g})`} />
      <circle cx={200 + r() * 800} cy={120 + r() * 200} r={46 + r() * 50} fill="#fff3cf" opacity="0.92" />
      {ridges.map((x, i) => <path key={i} d={x.d} fill={x.fill} />)}
    </g>
  )
}

// A photo's rotation and crop apply wherever it appears, because each view
// reads the same object. A reference can narrow that with its own crop.
export function Photo({ o, crop, full }) {
  const c = crop ?? visibleCrop(o)
  if (o.src && !full && !o.rot && !o.crop && !crop) {
    return <img className="photo" src={`/media/thumb/${o.src}`} alt={o.title} loading="lazy" decoding="async" />
  }
  const { w, h } = visibleSize(o, c)
  const inner = o.rot % 180 ? { w: h, h: w } : { w, h }
  const place = { 0: '', 90: `translate(${w} 0)`, 180: `translate(${w} ${h})`, 270: `translate(0 ${h})` }[o.rot]
  return (
    <svg className="photo" viewBox={`0 0 ${w} ${h}`} role="img" aria-label={o.title}>
      <g transform={`${place} rotate(${o.rot})`}>
        <svg width={inner.w} height={inner.h} viewBox={`${c.x} ${c.y} ${c.w} ${c.h}`} preserveAspectRatio="none">
          {o.src
            ? <>
                <image href={`/media/thumb/${o.src}`} width={BASE.w} height={BASE.h} preserveAspectRatio="none" />
                {full && <image href={`/media/photo/${o.src}`} width={BASE.w} height={BASE.h} preserveAspectRatio="none" />}
              </>
            : <Scene seed={o.seed} />}
        </svg>
      </g>
    </svg>
  )
}

export function Poster({ seed, src, title, sub }) {
  const r = rng(seed)
  const hue = Math.floor(r() * 360)
  return (
    <div className="poster" style={{ background: `linear-gradient(160deg, hsl(${hue} 55% 34%), hsl(${(hue + 60) % 360} 60% 16%))` }}>
      {src && <img className="still" src={`/media/poster/${src}`} alt="" loading="lazy" decoding="async" />}
      <strong>{title}</strong>
      {sub && <span>{sub}</span>}
    </div>
  )
}

// Stands in for a film: the light moves with the time, the palette with the chapter.
export function Film({ o, t }) {
  const f = t / o.dur
  const ch = o.chapters.findLastIndex((c) => c[1] <= t)
  const hue = (o.seed * 13 + ch * 67) % 360
  return (
    <svg className="film" viewBox="0 0 1600 900" role="img" aria-label={`${o.title}, chapter ${o.chapters[ch][0]}`}>
      <rect width="1600" height="900" fill={`hsl(${hue} 46% ${16 + f * 10}%)`} />
      <circle cx={200 + f * 1200} cy={520 - Math.sin(f * Math.PI) * 340} r="86" fill={`hsl(${(hue + 40) % 360} 90% 72%)`} />
      <path d={`M0 ${640 + ch * 16} Q 500 ${540 + ch * 30} 1000 ${660} T 1600 ${600} V900 H0 Z`} fill={`hsl(${(hue + 190) % 360} 38% 15%)`} />
    </svg>
  )
}

export function Bars({ seed, t, playing }) {
  const r = rng(seed)
  const heights = Array.from({ length: 56 }, (_, i) => {
    const a = 0.24 + r() * 0.76
    return playing ? a * (0.62 + 0.38 * Math.sin(t * 3 + i * 0.7)) : a * 0.5
  })
  return (
    <div className="bars" aria-hidden="true">
      {heights.map((h, i) => <i key={i} style={{ height: `${Math.round(h * 100)}%` }} />)}
    </div>
  )
}
