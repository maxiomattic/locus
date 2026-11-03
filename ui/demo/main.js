import './demo.css'
import { install, reset } from './backend.js'

install()
document.getElementById('reset').addEventListener('click', reset)

// Touch: the interface is keyboard-only, so these buttons press its keys and a tap
// opens what it lands on.
const press = (key) => window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
document.querySelector('.demo-touch').addEventListener('click', (e) => {
  if (e.target.dataset.key) press(e.target.dataset.key)
})
if (matchMedia('(pointer: coarse)').matches) {
  document.getElementById('root').addEventListener('click', (e) => {
    if (e.target.closest('.item, .row, .block')) setTimeout(() => press(' '))
  })
}

await import('../src/main.jsx')
