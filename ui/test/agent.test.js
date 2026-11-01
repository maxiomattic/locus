import test from 'node:test'
import assert from 'node:assert/strict'
import { explain, generalize, signature, stubModel, norm } from '../server/agent.mjs'

test('a plan keeps its signature across wording, values and objects', () => {
  const objs = { a: { type: 'photo' }, b: { type: 'photo' } }
  assert.equal(
    signature([{ do: 'set', id: 'a', fields: { rot: 90 } }], ['a'], objs),
    signature([{ do: 'set', id: 'b', fields: { rot: 180 } }], ['b'], objs),
  )
  assert.notEqual(
    signature([{ do: 'set', id: 'a', fields: { rot: 90 } }], ['a'], objs),
    signature([{ do: 'set', id: 'a', fields: { tag: 90 } }], ['a'], objs),
  )
})

test('a value is explained by what was there before', () => {
  const run = { text: 'rotate this' }
  const x = (samples) => explain('f', samples.map((s) => ({ run, ...s })), 'x')
  assert.deepEqual(x([{ value: 'red' }, { value: 'red' }]).expr, 'red')
  assert.deepEqual(x([{ value: 90, before: 0 }, { value: 0, before: 270 }]).expr, { mod: [{ plus: [{ get: [{ var: 'x' }, 'f'] }, 90] }, 360] })
  assert.deepEqual(x([{ value: 'old A', before: 'A' }, { value: 'old B', before: 'B' }]).expr, { concat: ['old ', { get: [{ var: 'x' }, 'f'] }] })
  assert.deepEqual(x([{ value: 'a', before: 'A' }, { value: 'b', before: 'B' }]).expr, { lower: { get: [{ var: 'x' }, 'f'] } })
  assert.equal(x([{ value: 'q', before: 'A' }, { value: 'zz', before: 'B' }]).ok, false)
})

test('a number the request names is a constant, one it doesn’t is relative', () => {
  const said = { text: 'set volume to 70' }
  const quiet = { text: 'rotate this' }
  assert.equal(explain('f', [{ value: 70, before: 0, run: said }, { value: 70, before: 0, run: said }], 'x').expr, 70)
  assert.deepEqual(explain('rot', [{ value: 90, before: 0, run: quiet }, { value: 90, before: 0, run: quiet }], 'x').expr, { mod: [{ plus: [{ get: [{ var: 'x' }, 'rot'] }, 90] }, 360] })
  assert.deepEqual(explain('n', [{ value: 5, before: 2, run: quiet }, { value: 5, before: 2, run: quiet }], 'x').expr, { plus: [{ get: [{ var: 'x' }, 'n'] }, 3] })
})

test('one differing word becomes a parameter', () => {
  const run = (text, id, word) => ({ text, selection: [id], ops: [{ do: 'set', id, fields: { tag: word } }], before: { [id]: {} } })
  const g = generalize(run('tag this red', 'a', 'red'), run('tag this blue', 'b', 'blue'))
  assert.equal(g.param, 2)
  assert.deepEqual(g.program[0].do[0].set.fields.obj.tag, { var: 'word' })
})

test('runs that don’t explain themselves are left to the model', () => {
  const run = (id, v) => ({ text: 'summarize this', selection: [id], ops: [{ do: 'set', id, fields: { sum: v } }], before: { [id]: { sum: '' } } })
  assert.equal(generalize(run('a', 'first'), run('b', 'other')), null)
})

test('stop words do not count', () => assert.deepEqual(norm('Rotate this photo, please'), ['rotate', 'photo']))
test('the stub model counts calls', async () => { const m = stubModel(); await m('REQUEST: how many'); assert.equal(m.calls, 1) })
