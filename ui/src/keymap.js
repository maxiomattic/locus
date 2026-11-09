// Every key the interface binds outside typing, as data. The engine looks a key
// up here and runs the command it names. The key sets and the ? panel below are
// worked out from these tables, so they can't disagree with what the keys do.
// Typing, the command bar and find take text, and read their few keys directly.

// Motions are named for their role, and each view decides what one means: a big
// step left is the start of a row on a grid, a word in text, a week in the calendar.
export const MOTIONS = new Set(['left', 'down', 'up', 'right', 'bigLeft', 'bigDown', 'bigUp', 'bigRight', 'start', 'end'])

export const NORMAL = {
  h: 'left', j: 'down', k: 'up', l: 'right',
  H: 'bigLeft', J: 'bigDown', K: 'bigUp', L: 'bigRight',
  g: 'start', G: 'end',
  ' ': 'activate', Escape: 'back',
  s: 'mark', S: 'range', a: 'all',
  i: 'edit', o: 'create',
  x: 'cut', c: 'copy', v: 'paste', z: 'undo', Z: 'redo', '.': 'repeat',
  ';': 'bar', '?': 'help', '/': 'find',
}

// Checked before NORMAL when Shift is held. A shifted letter is its own key above.
export const SHIFT = { ' ': 'newTile' }

export const ALT = { h: 'tileLeft', j: 'tileDown', k: 'tileUp', l: 'tileRight', q: 'closeTile' }

// Each opens the Home item whose launch field is that key.
export const LAUNCH = {
  n: 'Notes', p: 'Photos', m: 'Music', w: 'Movies', e: 'People',
  C: 'Calendar', M: 'Messages', d: 'Inbox', D: 'Tools',
}

// The key for each view's own commands. Views that never show together can share
// one: t is Today in the calendar and To-do in a note.
export const VIEW = {
  today: 't', endLess: '<', endMore: '>', repeat: 'y',
  rotate: 'r', crop: 'X', uncrop: 'u',
  kindT: 't', kindH: 'b', kindP: 'q',
  sortName: 'y', sortReverse: 'Y',
  approve: 'A', reject: 'R', send: 'A', setPeer: 'P',
  revert: 'u', restore: 'u',
  keepTheirs: 'O', keepMine: 'V',
  info: 'I', newTile: 'T',
}

const hit = (table, key) => (Object.hasOwn(table, key) ? table[key] : null)

// The command a key press names in normal mode, or null.
export function lookup(k) {
  if (k.alt) return hit(ALT, k.key)
  if (k.ctrl) return null
  return (k.shift && hit(SHIFT, k.key)) || hit(NORMAL, k.key)
}

export const isLaunch = (key) => Object.hasOwn(LAUNCH, key)

// ---------- Key sets ----------

export const RESERVED = new Set([...Object.keys(NORMAL), ...Object.keys(LAUNCH)].filter((c) => c.length === 1))

// These commit something that can't be taken back with a keystroke. They are
// never reused for anything else, so muscle memory can't fire one by accident.
export const CONSEQUENTIAL = new Set(['approve', 'reject', 'send'].map((c) => VIEW[c]))

// Keys the interface's own commands use in some view. A learned tool never takes
// one of these or a reserved key, so a key means one thing wherever it shows.
export const COMMAND_KEYS = new Set(Object.values(VIEW).filter((c) => !CONSEQUENTIAL.has(c)))

export const TOOL_KEYS = [...'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789']
  .filter((c) => !RESERVED.has(c) && !COMMAND_KEYS.has(c) && !CONSEQUENTIAL.has(c))

// ---------- Showing keys ----------

const NAMES = { ' ': 'Space', Escape: 'Esc' }
const keysIn = (table, ...commands) => commands
  .map((c) => Object.keys(table).find((key) => table[key] === c))
  .map((key) => NAMES[key] ?? key)
  .join(' ')

// The keys for these commands, as the interface writes them: 'h j k l'.
export const keysOf = (...commands) => keysIn(NORMAL, ...commands)

const row = (commands, what) => [keysOf(...commands.split(' ')), what]

// The grammar at a glance, for the ? panel.
export const HELP = [
  ['Move', [
    row('left down up right', 'left, down, up, right'),
    row('bigLeft bigDown bigUp bigRight', 'a big step'),
    row('start end', 'start, end'),
    row('find', 'find here'),
  ]],
  ['In and out', [
    row('activate', 'open, play, tick'),
    row('back', 'back'),
    [`⇧ ${keysIn(SHIFT, 'newTile')}`, 'open in a new tile'],
  ]],
  ['Select', [row('mark', 'mark one'), row('range', 'start a range'), row('all', 'all')]],
  ['Verbs', [
    row('edit', 'edit'), row('create', 'new'), row('cut', 'cut'), row('copy', 'copy'),
    row('paste', 'paste'), row('undo redo', 'undo, redo'), row('repeat', 'repeat'),
  ]],
  ['Apps', Object.entries(LAUNCH)],
  ['Everything else', [
    row('bar', 'command, or ask the agent'),
    [`${VIEW.approve} ${VIEW.reject}`, 'approve, reject'],
    [`Alt ${keysIn(ALT, 'tileLeft', 'tileDown', 'tileUp', 'tileRight')}`, 'between tiles'],
    [`Alt ${keysIn(ALT, 'closeTile')}`, 'close a tile'],
  ]],
]
