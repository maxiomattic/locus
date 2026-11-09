import { HELP } from './keymap.js'

// The grammar at a glance. The same keys mean the same thing in every view; the
// keys at the top right of a tile are that view's own.
export function Keys() {
  return (
    <aside className="keys" aria-label="Keys">
      <div className="keyshead"><span>Keys</span><span><kbd>?</kbd> hides</span></div>
      {HELP.map(([name, rows]) => (
        <section key={name}>
          <h3>{name}</h3>
          <dl>{rows.map(([k, what]) => <div key={k}><dt><kbd>{k}</kbd></dt><dd>{what}</dd></div>)}</dl>
        </section>
      ))}
      <p className="keysnote">Keys at the top right of a tile belong to that view, including tools the agent has learned.</p>
    </aside>
  )
}
