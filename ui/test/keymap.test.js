import test from 'node:test'
import assert from 'node:assert/strict'
import { ALT, COMMAND_KEYS, CONSEQUENTIAL, HELP, LAUNCH, MOTIONS, NORMAL, RESERVED, SHIFT, TOOL_KEYS, VIEW, keysOf, lookup } from '../src/keymap.js'

const sorted = (set) => [...set].sort()
const chars = (text) => sorted(new Set(text))

test('the key sets are worked out from the tables', () => {
  assert.deepEqual(sorted(RESERVED), chars('hjklHJKLgGsSaioxcvzZ.npmwedDCM;/? '))
  assert.deepEqual(sorted(CONSEQUENTIAL), chars('AR'))
  assert.deepEqual(sorted(COMMAND_KEYS), chars('tubqyYrXIOVTP<>'))
  assert.equal(TOOL_KEYS.join(''), 'BEFNQUWf0123456789')
})

test('a key means one thing wherever it shows', () => {
  const global = new Set([...Object.keys(NORMAL), ...Object.keys(LAUNCH)])
  assert.equal(global.size, Object.keys(NORMAL).length + Object.keys(LAUNCH).length)
  for (const [command, key] of Object.entries(VIEW)) assert.ok(!global.has(key), `${command} on ${key}`)
})

test('a consequential key is used only by consequential commands', () => {
  for (const [command, key] of Object.entries(VIEW)) {
    assert.equal(CONSEQUENTIAL.has(key), ['approve', 'reject', 'send'].includes(command), command)
  }
})

test('lookup follows the modifiers', () => {
  assert.equal(lookup({ key: 'h' }), 'left')
  assert.equal(lookup({ key: 'H', shift: true }), 'bigLeft')
  assert.equal(lookup({ key: ' ' }), 'activate')
  assert.equal(lookup({ key: ' ', shift: true }), 'newTile')
  assert.equal(lookup({ key: 'h', alt: true }), 'tileLeft')
  assert.equal(lookup({ key: 'x', alt: true }), null)
  assert.equal(lookup({ key: 'h', ctrl: true }), null)
  assert.equal(lookup({ key: 'constructor' }), null)
  assert.equal(lookup({ key: 'n' }), null)
})

test('every motion has a key', () => {
  for (const m of MOTIONS) assert.notEqual(keysOf(m), '', m)
  assert.equal(SHIFT[' '], 'newTile')
  assert.equal(ALT.q, 'closeTile')
})

test('the ? panel lists these keys', () => {
  assert.deepEqual(HELP, [
    ['Move', [['h j k l', 'left, down, up, right'], ['H J K L', 'a big step'], ['g G', 'start, end'], ['/', 'find here']]],
    ['In and out', [['Space', 'open, play, tick'], ['Esc', 'back'], ['⇧ Space', 'open in a new tile']]],
    ['Select', [['s', 'mark one'], ['S', 'start a range'], ['a', 'all']]],
    ['Verbs', [['i', 'edit'], ['o', 'new'], ['x', 'cut'], ['c', 'copy'], ['v', 'paste'], ['z Z', 'undo, redo'], ['.', 'repeat']]],
    ['Apps', [['n', 'Notes'], ['p', 'Photos'], ['m', 'Music'], ['w', 'Movies'], ['e', 'People'], ['C', 'Calendar'], ['M', 'Messages'], ['d', 'Inbox'], ['D', 'Tools']]],
    ['Everything else', [[';', 'command, or ask the agent'], ['A R', 'approve, reject'], ['Alt h j k l', 'between tiles'], ['Alt q', 'close a tile']]],
  ])
})
