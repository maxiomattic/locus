// Line icons for the doors on Home and other collections, drawn on a 24 grid.
const PATHS = {
  n: <><path d="M6 3h9l4 4v14H6z" /><path d="M14 3v5h5" /><path d="M9 12h7M9 16h7" /></>,
  p: <><rect x="3" y="5" width="18" height="14" rx="2" /><circle cx="9" cy="10" r="1.6" /><path d="M21 16l-5-5-8 8" /></>,
  m: <><path d="M9 18V5l11-2v13" /><circle cx="6.5" cy="18" r="2.5" /><circle cx="17.5" cy="16" r="2.5" /></>,
  w: <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M7 5v14M17 5v14M3 9.5h4M3 14.5h4M17 9.5h4M17 14.5h4" /></>,
  e: <><circle cx="9" cy="8" r="3.2" /><path d="M3 20c0-3.3 2.7-6 6-6s6 2.7 6 6" /><circle cx="17" cy="9" r="2.4" /><path d="M15.5 14.2c2.9-.3 5.5 1.9 5.5 5.8" /></>,
  d: <><path d="M3 13l3-8h12l3 8v6H3z" /><path d="M3 13h5l1.5 2.5h5L16 13h5" /></>,
  D: <path d="M15 3.5a5 5 0 0 0-4.6 6.9L3.5 17.3V20.5h3.2l6.9-6.9A5 5 0 0 0 20.5 9l-3.2 1.1-3.4-3.4z" />,
  C: <><rect x="3" y="5" width="18" height="16" rx="2" /><path d="M3 10h18M8 3v4M16 3v4" /><path d="M7.5 14h2M11 14h2M14.5 14h2M7.5 17.5h2M11 17.5h2" /></>,
  M: <path d="M4 5h16v11H9.5L4 20.5z" />,
  folder: <path d="M3 6h6l2 2h10v11H3z" />,
  album: <><rect x="3" y="5" width="18" height="14" rx="2" /><circle cx="9" cy="10" r="1.6" /><path d="M21 16l-5-5-8 8" /></>,
}

// Each app keeps one colour, so its door is easy to find by eye.
export const ACCENT = {
  n: '#8fb3ff', p: '#46d9c5', m: '#ff7fa8', w: '#c79bff', e: '#ffb070',
  d: '#7fd6ff', D: '#a8e07a', C: '#ff8f7a', M: '#6fe0a8',
}

export function iconFor(o) {
  if (o.launch && PATHS[o.launch]) return o.launch
  return o.accepts === 'photo' ? 'album' : 'folder'
}

export function Icon({ name, className = 'icon' }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {PATHS[name] ?? PATHS.folder}
    </svg>
  )
}
