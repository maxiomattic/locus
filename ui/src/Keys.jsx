// The grammar at a glance. The same keys mean the same thing in every view; the
// keys at the top right of a tile are that view's own.
const GROUPS = [
  ['Move', [['h j k l', 'left, down, up, right'], ['H J K L', 'a big step'], ['g G', 'start, end'], ['/', 'find here']]],
  ['In and out', [['Space', 'open, play, tick'], ['Esc', 'back'], ['⇧ Space', 'open in a new tile']]],
  ['Select', [['s', 'mark one'], ['S', 'start a range'], ['a', 'all']]],
  ['Verbs', [['i', 'edit'], ['o', 'new'], ['x', 'cut'], ['c', 'copy'], ['v', 'paste'], ['z Z', 'undo, redo'], ['.', 'repeat']]],
  ['Apps', [['n', 'Notes'], ['p', 'Photos'], ['m', 'Music'], ['w', 'Movies'], ['e', 'People'], ['C', 'Calendar'], ['M', 'Messages'], ['d', 'Inbox'], ['D', 'Tools']]],
  ['Everything else', [[';', 'command, or ask the agent'], ['A R', 'approve, reject'], ['Alt h j k l', 'between tiles'], ['Alt q', 'close a tile']]],
]

export function Keys() {
  return (
    <aside className="keys" aria-label="Keys">
      <div className="keyshead"><span>Keys</span><span><kbd>?</kbd> hides</span></div>
      {GROUPS.map(([name, rows]) => (
        <section key={name}>
          <h3>{name}</h3>
          <dl>{rows.map(([k, what]) => <div key={k}><dt><kbd>{k}</kbd></dt><dd>{what}</dd></div>)}</dl>
        </section>
      ))}
      <p className="keysnote">Keys at the top right of a tile belong to that view, including tools the agent has learned.</p>
    </aside>
  )
}
