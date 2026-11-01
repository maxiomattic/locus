import { createRoot } from 'react-dom/client'
import { App } from './App.jsx'
import { connect, toLocal } from './sync.js'
import './style.css'

const state = await fetch('/store/~/v1/state').then((r) => (r.ok ? r.json() : null), () => null)
const root = createRoot(document.getElementById('root'))
if (!state?.objects?.length) {
  root.render(<p style={{ padding: '2rem' }}>This ship has nothing in its store yet, or it can’t be reached.</p>)
} else {
  const store = Object.fromEntries(state.objects.filter((o) => !o.trashed).map((o) => [o.id, toLocal(o)]))
  root.render(<App store={store} sync={connect(store, state.ship)} />)
}
