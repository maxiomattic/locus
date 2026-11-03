import './demo.css'
import { install, reset, KEY } from './backend.js'

install()
// Commands only the demo has, offered in the command bar.
const first = !localStorage.getItem(KEY)
window.locusExtra = {
  start: first ? 'demo-welcome' : null,
  commands: [{ label: 'Reset the demo', run: 'page:reset' }, { label: 'Credits', run: 'page:credits' }],
  run: (what) => (what === 'reset' ? reset() : what === 'credits' ? location.assign('/credits.html') : null),
}

// Touch: the interface is keyboard-only, so these buttons press its keys and a tap
// opens what it lands on.
const press = (key) => window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
document.querySelector('.demo-touch').addEventListener('click', (e) => {
  if (e.target.dataset.key) press(e.target.dataset.key)
})
if (matchMedia('(pointer: coarse)').matches) {
  document.getElementById('root').addEventListener('click', (e) => {
    if (e.target.closest('.item, .row, .block, .event')) setTimeout(() => press(' '))
  })
}

await import('../src/main.jsx')
